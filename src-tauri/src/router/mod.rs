// router — WS 通信 + 消息分发
//
// 从 pilot/ 拆出的通信层:
//   dispatch.rs    ← 消息路由 (统一分发到 AppServices)
//   connector.rs   ← WS 连接管理
//   protocol.rs    ← JSON-RPC 编解码
//   config.rs      ← 连接配置

pub mod dispatch;
pub mod connector;
pub mod protocol;
pub mod config;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use config::PilotConfig;
use connector::Connector;
use dispatch::MessageRouter;

use crate::ui::auth::AuthManager;
use crate::infra::store::LocalStore;
use crate::infra::guard::ToolGuard;
use crate::infra::audit::AuditLog;
use crate::infra::mcp::McpBridge;
use crate::infra::runtime;
use crate::services::AppServices;

/// 全局状态 — 注入到所有 Tauri Commands
pub struct AppState {
    pub config: Arc<PilotConfig>,
    pub auth: Arc<RwLock<AuthManager>>,
    pub connector: Arc<Connector>,
    pub guard: Arc<RwLock<ToolGuard>>,
    pub audit: Arc<AuditLog>,
    pub store: Arc<LocalStore>,
    pub mcp: Arc<RwLock<McpBridge>>,
    pub app: Arc<AppServices>,
}

impl AppState {
    /// 初始化所有子模块, spawn connector 后台任务
    pub async fn init(data_dir: PathBuf) -> Self {
        // 1. 配置
        let config = Arc::new(PilotConfig::from_env(data_dir.clone()));
        log::info!(
            "[AppState] 初始化 → {} ({})",
            config.server_host,
            if config.is_local() { "开发" } else { "生产" }
        );

        // 2. 存储
        let store = Arc::new(LocalStore::new(&data_dir));

        // 3. 认证
        let auth = Arc::new(RwLock::new(AuthManager::new(&data_dir)));

        // 4. 从 store 读取保存的配置
        let guard_mode = store
            .get_str("settings", "guard_mode")
            .unwrap_or_else(|| config.guard_mode.clone());

        // 5. 安全门控
        let mut guard_inner = ToolGuard::new(&guard_mode, &data_dir, &config.os_name);
        guard_inner.load_remembered().await;
        let guard = Arc::new(RwLock::new(guard_inner));

        // 6. 审计
        let audit = Arc::new(AuditLog::new(&data_dir));
        audit.init().await;

        // 7. 运行时管理
        let _rt = Arc::new(runtime::RuntimeManager::new(&data_dir));

        // 7.1 CLI 工具下载 (rg, fd, jq, yq) — 后台非阻塞
        {
            let data_dir_clone = data_dir.clone();
            let oss_url = config.tools_oss_url.clone();
            log::info!("[AppState] CLI 工具下载任务启动 (oss={})", oss_url);
            tokio::spawn(async move {
                let mgr = crate::infra::tools::ToolsManager::new(&data_dir_clone, &oss_url);
                if let Err(e) = mgr.ensure_ready().await {
                    log::warn!("[AppState] CLI 工具下载失败 (非致命): {e}");
                } else {
                    log::info!("[AppState] CLI 工具下载完成 ✅");
                }
            });
        }

        // 8. MCP Bridge
        let config_dir = data_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("config"))
            .unwrap_or_else(|| data_dir.join("config"));
        let mcp = Arc::new(RwLock::new(McpBridge::load(
            &config_dir,
            Some(_rt.clone()),
        )));

        // 9. AppServices (唯一业务层)
        let app = Arc::new(AppServices::new(guard.clone(), audit.clone()));

        // 10. 路由 (只依赖 AppServices)
        let router = Arc::new(MessageRouter::new(app.clone()));

        // 11. 连接器
        let connector = Arc::new(Connector::new());

        // 12. spawn 连接器后台任务
        {
            let conn = connector.clone();
            let cfg = config.clone();
            let a = auth.clone();
            let r = router.clone();
            let s = store.clone();
            let ap = app.clone();
            tokio::spawn(async move {
                conn.run(cfg, a, r, s, ap).await;
            });
        }

        // 13. 空闲清理定时器
        {
            let ap = app.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    ap.browser.cleanup_if_idle(Duration::from_secs(300)).await;
                    let _ = ap.shell.cleanup_all().await;
                }
            });
        }

        log::info!("[AppState] 初始化完成");

        Self {
            config,
            auth,
            connector,
            guard,
            audit,
            store,
            mcp,
            app,
        }
    }
}
