// WebSocket 连接管理
//
// 职责:
//   · 管理 WebSocket 连接生命周期 (连接 / 心跳 / 重连)
//   · 分发消息到 MessageRouter
//   · 退避重连策略

use std::sync::Arc;
use std::time::{Duration, Instant};
use futures_util::{StreamExt, stream::SplitSink};
use tokio::sync::{Notify, RwLock, mpsc};

use crate::services::AppServices;
use tokio_tungstenite::{MaybeTlsStream, tungstenite::Message};

/// 具体的 WebSocket Sink 类型 (connect_async 返回)
type WsSink = SplitSink<
    tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

use crate::ui::auth::AuthManager;
use super::config::PilotConfig;
use super::protocol::{BridgeSender, IncomingMessage, JsonRpcResponse};
use super::dispatch::MessageRouter;
use crate::infra::store::LocalStore;

// ── 连接状态 ──────────────────────────────

/// WebSocket 连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum ConnState {
    Disconnected,
    Connecting,
    Connected,
}

impl std::fmt::Display for ConnState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "disconnected"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
        }
    }
}

// ── Connector ──────────────────────────────

/// WebSocket 连接器 — 管理到后端的持久连接
pub struct Connector {
    state: RwLock<ConnState>,
    wake_notify: Arc<Notify>,
    start_time: Instant,
    server_reachable: RwLock<bool>,
}

impl Connector {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ConnState::Disconnected),
            wake_notify: Arc::new(Notify::new()),
            start_time: Instant::now(),
            server_reachable: RwLock::new(false),
        }
    }

    pub async fn state(&self) -> ConnState {
        self.state.read().await.clone()
    }

    pub fn uptime(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    pub async fn server_reachable(&self) -> bool {
        *self.server_reachable.read().await
    }

    /// 唤醒重连 (auth 保存/清除后调用)
    pub fn wake(&self) {
        self.wake_notify.notify_one();
    }

    /// 强制断开 (logout 用)
    pub async fn disconnect(&self) {
        *self.state.write().await = ConnState::Disconnected;
        self.wake_notify.notify_one();
    }

    // ── 主循环 ──────────────────────────────

    /// 主循环 — spawn 到 tokio task
    pub async fn run(
        self: Arc<Self>,
        config: Arc<PilotConfig>,
        auth: Arc<RwLock<AuthManager>>,
        router: Arc<MessageRouter>,
        store: Arc<LocalStore>,
        app: Arc<AppServices>,
    ) {
        let mut retry_delay = Duration::from_secs(1);

        loop {
            // 启动/重连时先检查服务可达性
            self.check_server_health(&config).await;

            // 等待 Token
            let token = match self.wait_for_token(&auth, &config).await {
                Some(t) => t,
                None => {
                    retry_delay = Duration::from_millis(500);
                    continue;
                }
            };

            // 尝试连接
            *self.state.write().await = ConnState::Connecting;
            let device_id = auth.read().await.device_id.clone();
            let url = format!("{}?token={}&device_id={}", config.ws_url, token, device_id);
            log::info!("[Connector] 连接 {} (device={})...", config.ws_url, device_id);

            match tokio_tungstenite::connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    *self.state.write().await = ConnState::Connected;
                    *self.server_reachable.write().await = true;
                    retry_delay = Duration::from_secs(1);
                    log::info!("[Connector] ✅ 已连接");

                    // 获取用户信息
                    auth.write().await.fetch_user_info(&config.http_url).await;

                    // 运行会话 (阻塞直到断开)
                    self.run_session(ws_stream, &config, &auth, &router, &store, &app).await;
                }
                Err(e) => {
                    log::warn!("[Connector] ❌ 连接失败: {e}");
                    self.check_server_health(&config).await;
                }
            }

            *self.state.write().await = ConnState::Disconnected;

            // 退避重连
            if auth.read().await.get_token().is_none() {
                continue;
            }

            log::info!("[Connector] {:.0}s 后重连...", retry_delay.as_secs_f64());
            tokio::select! {
                _ = tokio::time::sleep(retry_delay) => {}
                _ = self.wake_notify.notified() => {
                    log::info!("[Connector] 被唤醒, 立即重连");
                }
            }
            retry_delay = (retry_delay * 2).min(Duration::from_secs(60));
        }
    }

    // ── 会话管理 ──────────────────────────────

    /// 等待 Token — 无 Token 时每 30s 做一次 health check
    async fn wait_for_token(
        &self,
        auth: &Arc<RwLock<AuthManager>>,
        config: &PilotConfig,
    ) -> Option<String> {
        loop {
            let token = {
                let a = auth.read().await;
                a.get_token().map(|s| s.to_string())
            };

            match token {
                Some(t) if !t.is_empty() => return Some(t),
                _ => {
                    log::info!("[Connector] 无 Token, 等待授权...");
                    *self.state.write().await = ConnState::Disconnected;

                    // 每 30s 超时检查一次 health, 或被 wake 唤醒
                    tokio::select! {
                        _ = self.wake_notify.notified() => {
                            // 被唤醒 (可能 token 已保存)
                        }
                        _ = tokio::time::sleep(Duration::from_secs(30)) => {
                            self.check_server_health(config).await;
                        }
                    }

                    // 再检查一次 token
                    let has_token = auth.read().await.get_token().is_some();
                    if !has_token {
                        continue; // 继续等待
                    }
                }
            }
            // token 已获取, 返回
            let a = auth.read().await;
            return a.get_token().map(|s| s.to_string());
        }
    }

    /// 运行已连接的 WebSocket 会话
    async fn run_session(
        &self,
        ws_stream: tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
        config: &PilotConfig,
        auth: &Arc<RwLock<AuthManager>>,
        router: &Arc<MessageRouter>,
        store: &LocalStore,
        app: &Arc<AppServices>,
    ) {
        let (sink, mut stream) = ws_stream.split();
        let mut sender = BridgeSender::new(sink);

        // 发送 hello (含浏览器页面状态)
        self.send_hello(&mut sender, auth, config, store, app).await;

        // 心跳
        let (ping_tx, mut ping_rx) = mpsc::channel::<()>(1);
        let heartbeat = self.spawn_heartbeat(ping_tx);

        // 响应通道: handler 执行完后把响应发到这里, 消息循环异步发出
        let (resp_tx, mut resp_rx) = mpsc::channel::<JsonRpcResponse>(32);

        // 消息循环
        loop {
            tokio::select! {
                msg = stream.next() => {
                    match msg {
                        Some(Ok(m)) => {
                            if !self.handle_ws_message(m, &mut sender, router, &resp_tx).await {
                                break; // 收到 Close 或需要断开
                            }
                        }
                        Some(Err(e)) => {
                            log::warn!("[Connector] WS 错误: {e}");
                            break;
                        }
                        None => break, // stream 结束
                    }
                }
                _ = ping_rx.recv() => {
                    sender.ping().await;
                    sender.raw_send(Message::Ping(vec![])).await;
                }
                // handler 执行完 → 发送响应
                resp = resp_rx.recv() => {
                    if let Some(r) = resp {
                        sender.respond(r).await;
                    }
                }
            }

            // 外部断开检查
            if *self.state.read().await != ConnState::Connected {
                break;
            }
        }

        heartbeat.abort();
    }

    /// 发送 hello 握手
    async fn send_hello(
        &self,
        sender: &mut BridgeSender<WsSink>,
        auth: &Arc<RwLock<AuthManager>>,
        config: &PilotConfig,
        store: &LocalStore,
        app: &Arc<AppServices>,
    ) {
        let (device_id, device_name) = {
            let a = auth.read().await;
            (a.device_id.clone(), a.device_name.clone())
        };
        let workspace = store.get_str("settings", "workspace")
            .unwrap_or_else(|| config.workspace.clone());

        // 通过 AppServices 获取状态
        let browser_state = app.get_browser_state().await;
        let shell_sessions = app.get_shell_sessions().await;
        let skills = app.get_skill_names().await;

        sender.hello(
            &device_id, &device_name,
            &config.os_name, &workspace,
            &config.version(),
            &["shell", "file", "browser", "mcp"],
            browser_state,
            shell_sessions,
            skills,
        ).await;
    }

    /// 启动心跳 task
    fn spawn_heartbeat(&self, ping_tx: mpsc::Sender<()>) -> tokio::task::JoinHandle<()> {
        let wake = self.wake_notify.clone();
        let state = Arc::new(self.state.read());
        // 使用 Arc<Self> 无法在此处, 所以用 Notify 检测
        let wake2 = self.wake_notify.clone();
        drop(state); // 释放读锁

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(25)) => {
                        if ping_tx.send(()).await.is_err() {
                            break; // channel 关闭 = session 结束
                        }
                    }
                    _ = wake.notified() => break,
                }
            }
            drop(wake2);
        })
    }

    // ── 消息处理 ──────────────────────────────

    /// 处理单条 WebSocket 消息 (返回 false 表示断开)
    async fn handle_ws_message(
        &self,
        msg: Message,
        sender: &mut BridgeSender<WsSink>,
        router: &Arc<MessageRouter>,
        resp_tx: &mpsc::Sender<JsonRpcResponse>,
    ) -> bool {
        match msg {
            Message::Text(text) => {
                self.handle_text_message(&text, router, resp_tx).await;
                true
            }
            Message::Ping(data) => {
                sender.raw_send(Message::Pong(data)).await;
                true
            }
            Message::Close(_) => {
                log::info!("[Connector] 收到关闭帧");
                false
            }
            _ => true, // Pong, Binary 等忽略
        }
    }

    /// 处理 JSON-RPC 文本消息 — 非阻塞: spawn handler, 响应通过 channel 发回
    async fn handle_text_message(
        &self,
        text: &str,
        router: &Arc<MessageRouter>,
        resp_tx: &mpsc::Sender<JsonRpcResponse>,
    ) {
        let incoming = match serde_json::from_str::<IncomingMessage>(text) {
            Ok(m) if m.is_request() => m,
            _ => return,
        };

        let id = incoming.id.as_deref().unwrap_or("").to_string();
        let method = incoming.method.as_deref().unwrap_or("").to_string();
        let params = incoming.params.clone().unwrap_or(serde_json::json!({}));

        // 非阻塞: spawn handler, 执行完通过 resp_tx 发送响应
        let router_clone = router.clone();
        let tx = resp_tx.clone();
        tokio::spawn(async move {
            let resp = match tokio::task::spawn(async move {
                router_clone.handle_request(&id, &method, &params).await
            }).await {
                Ok(r) => r,
                Err(e) => {
                    log::error!("[Connector] handler panic: {e}");
                    let fallback_id = incoming.id.as_deref().unwrap_or("");
                    JsonRpcResponse::error(
                        fallback_id,
                        -32603, &format!("internal error: {e}"),
                    )
                }
            };
            let _ = tx.send(resp).await;
        });
    }

    /// HTTP 健康检查
    async fn check_server_health(&self, config: &PilotConfig) {
        let reachable = reqwest::Client::new()
            .get(format!("{}/health", config.http_url))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .is_ok();
        *self.server_reachable.write().await = reachable;
    }
}
