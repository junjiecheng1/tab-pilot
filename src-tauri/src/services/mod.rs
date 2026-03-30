// services — 业务服务层
//
// 对应 Python app/services/
// 注入 guard (安全门控) + audit (审计日志) 作为横切能力

pub mod auth;
pub mod browser;
pub mod browser_sdk;
pub mod shell;
pub mod file;
pub mod jupyter;
pub mod nodejs;
pub mod sandbox;
pub mod mcp_client;
pub mod skills;
pub mod session_pool;
pub mod toolkit_dispatch;
pub mod editor_manager;

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;

use crate::core::error::{ServiceError, ServiceResult};
use crate::infra::audit::AuditLog;
use crate::infra::guard::ToolGuard;

/// 应用服务容器 — 持有所有服务实例 + 横切能力
///
/// guard + audit 注入后, 所有 service 共享同一份, 无需各自持有
pub struct AppServices {
    // ── 横切能力 ──────────────────────────
    pub guard: Arc<RwLock<ToolGuard>>,
    pub audit: Arc<AuditLog>,

    // ── 业务服务 ──────────────────────────
    pub config: crate::core::config::AppConfig,
    pub auth: auth::AuthService,
    pub browser: browser::BrowserService,
    pub browser_sdk: browser_sdk::BrowserSdkService,
    pub shell: shell::ShellService,
    pub file: file::FileService,
    pub jupyter: jupyter::JupyterService,
    pub nodejs: nodejs::NodeJsService,
    pub sandbox: sandbox::SandboxService,
    pub mcp_client: mcp_client::McpClient,
    pub skills: RwLock<skills::SkillService>,
    pub editor_manager: editor_manager::EditorManagerService,
}

impl AppServices {
    /// 初始化 — 注入 guard + audit
    pub fn new(guard: Arc<RwLock<ToolGuard>>, audit: Arc<AuditLog>) -> Self {
        let config = crate::core::config::AppConfig::from_env();
        let mcp_client = mcp_client::McpClient::load(&config.mcp_servers_config);

        log::info!("[AppServices] 初始化完成 (guard + audit 已注入)");

        Self {
            guard,
            audit,
            config,
            auth: auth::AuthService::new(),
            browser: browser::BrowserService::new(),
            browser_sdk: browser_sdk::BrowserSdkService::new(),
            shell: shell::ShellService::new(),
            file: file::FileService::new(),
            jupyter: jupyter::JupyterService::new(),
            nodejs: nodejs::NodeJsService::new(),
            sandbox: sandbox::SandboxService::new(),
            mcp_client,
            skills: RwLock::new(skills::SkillService::new()),
            editor_manager: editor_manager::EditorManagerService::new(),
        }
    }

    /// WS 请求分发 — 按命名空间路由
    ///
    /// 格式: `app/<service>.<action>`
    ///
    /// 对于 shell 和 file 操作, 自动执行:
    ///   1. guard.check() → 安全门控
    ///   2. audit.log_start() → 记录开始
    ///   3. service.handle() → 执行业务
    ///   4. audit.log_finish() → 记录结果
    pub async fn handle_request(
        &self,
        method: &str,
        params: Value,
    ) -> ServiceResult {
        let path = method.strip_prefix("app/").unwrap_or(method);

        let (service, action) = path
            .split_once('.')
            .ok_or_else(|| ServiceError::bad_request(format!(
                "格式错误: {method} (需要 app/<service>.<action>)"
            )))?;

        // 需要 guard + audit 的服务
        match service {
            "shell" => self.guarded_shell(action, params).await,
            "file" => self.guarded_file(action, params).await,
            "browser" => self.audited_browser(action, params).await,

            // 不需要 guard 的服务
            "auth" => self.auth.handle(action, params).await,
            "browser_sdk" => self.browser_sdk.handle(action, params, &self.browser).await,
            "jupyter" => self.jupyter.handle(action, params).await,
            "nodejs" => self.nodejs.handle(action, params).await,
            "sandbox" => self.sandbox.handle(action, params).await,
            "mcp_client" => self.handle_mcp_client(action, &params),
            "skills" => {
                self.skills
                    .write()
                    .await
                    .handle(action, params)
                    .map_err(|e| ServiceError::internal(e.to_string()))
            }
            "editor" => self.editor_manager.handle(action, params).await,
            _ => Err(ServiceError::bad_request(format!("未知服务: {service}"))),
        }
    }

    // ── Shell: guard + audit ──────────────────

    async fn guarded_shell(&self, action: &str, params: Value) -> ServiceResult {
        use crate::infra::guard::GuardDecision;
        use std::time::Instant;

        // exec 操作需要安全门控
        if action == "exec" {
            let decision = {
                let guard = self.guard.read().await;
                guard.check("shell", &params)
            };

            match decision {
                GuardDecision::Deny => {
                    // 审计: 记录被拒绝
                    self.audit.log(
                        "shell", action, &params, "denied by guard",
                        -1, 0.0, "deny",
                    ).await;
                    return Err(ServiceError::forbidden("命令被安全门控拒绝"));
                }
                GuardDecision::Confirm => {
                    // 审计: 记录需确认
                    self.audit.log(
                        "shell", action, &params, "needs confirmation",
                        -1, 0.0, "confirm",
                    ).await;
                    return Err(ServiceError::forbidden("命令需要用户确认"));
                }
                GuardDecision::Allow => {}
            }
        }

        // 审计: 开始
        let t0 = Instant::now();
        let log_id = self.audit.log_start("shell", action, &params, "allow").await;

        // 执行
        let result = self.shell.handle(action, params).await;

        // 审计: 结束
        let duration = t0.elapsed().as_secs_f64();
        if let Some(id) = log_id {
            let (preview, exit_code, status) = match &result {
                Ok(v) => {
                    let s = serde_json::to_string(v).unwrap_or_default();
                    (s, 0, "completed")
                }
                Err(e) => (e.to_string(), 1, "failed"),
            };
            self.audit.log_finish(id, &preview, exit_code, duration, status).await;
        }

        result
    }

    // ── File: guard + audit ──────────────────

    async fn guarded_file(&self, action: &str, params: Value) -> ServiceResult {
        use crate::infra::guard::GuardDecision;
        use std::time::Instant;

        // 写操作需要安全门控
        let is_write = matches!(action, "write" | "append" | "delete" | "move" | "rename" | "mkdir");
        if is_write {
            let decision = {
                let guard = self.guard.read().await;
                guard.check("file", &params)
            };

            match decision {
                GuardDecision::Deny => {
                    self.audit.log(
                        "file", action, &params, "denied by guard",
                        -1, 0.0, "deny",
                    ).await;
                    return Err(ServiceError::forbidden("文件操作被安全门控拒绝"));
                }
                GuardDecision::Confirm => {
                    self.audit.log(
                        "file", action, &params, "needs confirmation",
                        -1, 0.0, "confirm",
                    ).await;
                    return Err(ServiceError::forbidden("文件操作需要用户确认"));
                }
                GuardDecision::Allow => {}
            }
        }

        // 审计
        let t0 = Instant::now();
        let log_id = self.audit.log_start("file", action, &params, "allow").await;

        let result = self.file.handle(action, params).await;

        let duration = t0.elapsed().as_secs_f64();
        if let Some(id) = log_id {
            let (preview, exit_code, status) = match &result {
                Ok(v) => {
                    let s = serde_json::to_string(v).unwrap_or_default();
                    (s, 0, "completed")
                }
                Err(e) => (e.to_string(), 1, "failed"),
            };
            self.audit.log_finish(id, &preview, exit_code, duration, status).await;
        }

        result
    }

    // ── Browser: audit only ──────────────────

    async fn audited_browser(&self, action: &str, params: Value) -> ServiceResult {
        use std::time::Instant;

        let t0 = Instant::now();
        let log_id = self.audit.log_start("browser", action, &params, "allow").await;

        let result = self.browser.handle(action, params).await;

        let duration = t0.elapsed().as_secs_f64();
        if let Some(id) = log_id {
            let (preview, exit_code, status) = match &result {
                Ok(v) => {
                    let s = serde_json::to_string(v).unwrap_or_default();
                    (s, 0, "completed")
                }
                Err(e) => (e.to_string(), 1, "failed"),
            };
            self.audit.log_finish(id, &preview, exit_code, duration, status).await;
        }

        result
    }

    // ── MCP Client ──────────────────────────

    fn handle_mcp_client(&self, action: &str, params: &Value) -> ServiceResult {
        match action {
            "list_servers" => Ok(self.mcp_client.to_json()),
            "get_server" => {
                let name = params["name"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 name"))?;
                match self.mcp_client.get_server(name) {
                    Some(cfg) => serde_json::to_value(cfg)
                        .map_err(|e| ServiceError::internal(e.to_string())),
                    None => Err(ServiceError::not_found(format!("服务器不存在: {name}"))),
                }
            }
            _ => Err(ServiceError::bad_request(format!("未知 mcp_client 操作: {action}"))),
        }
    }

    // ── 状态查询 (供 connector hello 使用) ──────────

    /// 获取浏览器状态 (hello 消息)
    pub async fn get_browser_state(&self) -> Option<serde_json::Value> {
        self.browser.get_browser_state().await
    }

    /// 获取活跃 shell 会话信息 (hello 消息)
    pub async fn get_shell_sessions(&self) -> Vec<serde_json::Value> {
        match self.shell.list_sessions().await {
            Ok(v) => v.get("sessions")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default(),
            Err(_) => vec![],
        }
    }

    /// 获取已注册的 skill 名称列表 (hello 消息)
    pub async fn get_skill_names(&self) -> Vec<String> {
        let svc = self.skills.read().await;
        let collection = svc.list_metadata(None);
        collection.skills.into_iter().map(|s| s.name).collect()
    }

    /// 关闭所有服务
    pub async fn shutdown(&self) {
        log::info!("[AppServices] 正在关闭...");
        self.browser.shutdown().await;
        let _ = self.shell.cleanup_all().await;
        let _ = self.jupyter.cleanup_all().await;
        log::info!("[AppServices] 已关闭");
    }
}
