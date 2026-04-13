// JSON-RPC 2.0 协议编解码

use serde::{Deserialize, Serialize};

/// Bridge 协议方法名常量
pub mod method {
    /// TabPilot → 后端: 握手
    pub const HELLO: &str = "bridge/hello";
    /// TabPilot → 后端: 心跳
    pub const PING: &str = "bridge/ping";
    /// TabPilot → 后端: MCP 工具列表更新
    pub const MCP_UPDATED: &str = "bridge/mcp_updated";
    /// 后端 → TabPilot: shell runtime
    pub const RUNTIME_SHELL_CREATE_SESSION: &str = "runtime/shell.create_session";
    pub const RUNTIME_SHELL_UPDATE_SESSION: &str = "runtime/shell.update_session";
    pub const RUNTIME_SHELL_EXEC: &str = "runtime/shell.exec";
    pub const RUNTIME_SHELL_VIEW: &str = "runtime/shell.view";
    pub const RUNTIME_SHELL_WAIT: &str = "runtime/shell.wait";
    pub const RUNTIME_SHELL_WRITE: &str = "runtime/shell.write";
    pub const RUNTIME_SHELL_KILL: &str = "runtime/shell.kill";
    pub const RUNTIME_SHELL_LIST_SESSIONS: &str = "runtime/shell.list_sessions";
    pub const RUNTIME_SHELL_CLEANUP_SESSION: &str = "runtime/shell.cleanup_session";
    pub const RUNTIME_SHELL_CLEANUP_ALL_SESSIONS: &str = "runtime/shell.cleanup_all_sessions";
}

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

impl JsonRpcRequest {
    pub fn new(method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id: uuid::Uuid::new_v4().to_string(),
            method: method.to_string(),
            params,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn success(id: &str, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id: id.to_string(),
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: &str, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: "2.0",
            id: id.to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.to_string(),
            }),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    pub params: serde_json::Value,
}

impl JsonRpcNotification {
    pub fn new(method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// 解析 JSON-RPC 消息
#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    pub id: Option<String>,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

impl IncomingMessage {
    pub fn is_request(&self) -> bool {
        self.id.is_some() && self.method.is_some()
    }

    pub fn is_notification(&self) -> bool {
        self.method.is_some() && self.id.is_none()
    }

    pub fn is_response(&self) -> bool {
        self.id.is_some() && self.method.is_none()
    }
}

// ═══════════════════════════════════════════════════════════
// BridgeSender — 类似 JS api 封装的 WS 发送器
// ═══════════════════════════════════════════════════════════

use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;

/// WS 发送器 — 封装所有协议发送逻辑
///
/// ```rust
/// sender.hello(&device_id, &device_name, &config).await;
/// sender.respond(JsonRpcResponse::success(id, result)).await;
/// ```
pub struct BridgeSender<S> {
    sink: S,
}

impl<S> BridgeSender<S>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    pub fn new(sink: S) -> Self {
        Self { sink }
    }

    /// 发送 hello 握手
    pub async fn hello(
        &mut self,
        device_id: &str,
        device_name: &str,
        os: &str,
        workspace: &str,
        version: &str,
        capabilities: &[&str],
        browser_state: Option<serde_json::Value>,
        shell_sessions: Vec<serde_json::Value>,
        skills: Vec<String>,
    ) {
        let mut params = serde_json::json!({
            "device_id": device_id,
            "device_name": device_name,
            "os": os,
            "workspace": workspace,
            "version": version,
            "capabilities": capabilities,
        });
        if let Some(bs) = browser_state {
            params["browser_state"] = bs;
        }
        if !shell_sessions.is_empty() {
            params["shell_sessions"] = serde_json::json!(shell_sessions);
        }
        if !skills.is_empty() {
            params["skills"] = serde_json::json!(skills);
        }
        self.notify(method::HELLO, params).await;
    }

    /// 发送 ping
    pub async fn ping(&mut self) {
        self.notify(method::PING, serde_json::json!({})).await;
    }

    /// 发送 MCP 工具更新
    pub async fn mcp_updated(&mut self, mcp_tools: serde_json::Value) {
        self.notify(
            method::MCP_UPDATED,
            serde_json::json!({
                "mcp_tools": mcp_tools,
            }),
        )
        .await;
    }

    /// 发送 JSON-RPC 响应 (工具调用结果)
    pub async fn respond(&mut self, resp: JsonRpcResponse) {
        let _ = self.sink.send(Message::Text(resp.to_json())).await;
    }

    /// 发送任意通知 (底层方法)
    pub async fn notify(&mut self, method: &str, params: serde_json::Value) {
        let msg = JsonRpcNotification::new(method, params);
        let _ = self.sink.send(Message::Text(msg.to_json())).await;
    }

    /// 发送原始 WebSocket 帧 (Ping/Pong)
    pub async fn raw_send(&mut self, msg: Message) {
        let _ = self.sink.send(msg).await;
    }
}
