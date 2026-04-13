// McpBridge — 统一管理 HTTP + stdio 会话

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::config::{McpConfig, McpToolInfo, McpTransport};
use super::session::{HttpSession, StdioSession};
use crate::infra::runtime::RuntimeManager;

/// MCP Bridge — 管理所有 MCP 服务器连接
pub struct McpBridge {
    /// 远程 HTTP 会话
    http_sessions: HashMap<String, HttpSession>,
    /// 本地 stdio 会话
    stdio_sessions: HashMap<String, StdioSession>,
    /// 待启动的 stdio 配置 (延迟启动)
    stdio_configs: HashMap<String, (String, Vec<String>, HashMap<String, String>)>,
    /// 已发现的工具
    tools: Vec<McpToolInfo>,
    /// Node 运行时 (stdio 需要)
    runtime: Option<Arc<RuntimeManager>>,
}

impl McpBridge {
    /// 从配置文件加载
    pub fn load(config_dir: &Path, runtime: Option<Arc<RuntimeManager>>) -> Self {
        let config = Self::read_config(config_dir);
        let mut http_sessions = HashMap::new();
        let mut stdio_configs = HashMap::new();

        if let Some(cfg) = config {
            for (name, transport) in cfg.servers {
                match transport {
                    McpTransport::Http { url } => {
                        http_sessions.insert(name.clone(), HttpSession::new(&url));
                        log::info!("[MCP] HTTP 服务器: {} → {}", name, url);
                    }
                    McpTransport::Stdio { command, args, env } => {
                        stdio_configs.insert(name.clone(), (command.clone(), args, env));
                        log::info!("[MCP] stdio 服务器: {} → {}", name, command);
                    }
                }
            }
        }

        Self {
            http_sessions,
            stdio_sessions: HashMap::new(),
            stdio_configs,
            tools: Vec::new(),
            runtime,
        }
    }

    fn read_config(config_dir: &Path) -> Option<McpConfig> {
        let dev_path = config_dir.join("mcporter.dev.json");
        let path = config_dir.join("mcporter.json");

        let file = if dev_path.exists() {
            dev_path
        } else if path.exists() {
            path
        } else {
            log::info!("[MCP] 无 mcporter.json 配置");
            return None;
        };

        match std::fs::read_to_string(&file) {
            Ok(content) => match serde_json::from_str::<McpConfig>(&content) {
                Ok(cfg) => {
                    log::info!(
                        "[MCP] 加载配置: {:?} ({} 个服务器)",
                        file,
                        cfg.servers.len()
                    );
                    Some(cfg)
                }
                Err(e) => {
                    log::warn!("[MCP] 配置解析失败: {}", e);
                    None
                }
            },
            Err(e) => {
                log::warn!("[MCP] 读取配置失败: {}", e);
                None
            }
        }
    }

    /// 初始化所有服务器并收集工具
    pub async fn discover_tools(&mut self) -> Vec<McpToolInfo> {
        let mut all_tools = Vec::new();

        // HTTP 服务器
        let http_names: Vec<String> = self.http_sessions.keys().cloned().collect();
        for name in http_names {
            if let Some(session) = self.http_sessions.get_mut(&name) {
                match Self::init_and_list(session, &name).await {
                    Ok(tools) => all_tools.extend(tools),
                    Err(e) => log::warn!("[MCP] {} 失败: {}", name, e),
                }
            }
        }

        // stdio 服务器 — 延迟启动
        let stdio_names: Vec<String> = self.stdio_configs.keys().cloned().collect();
        for name in stdio_names {
            match self.spawn_stdio(&name).await {
                Ok(tools) => all_tools.extend(tools),
                Err(e) => log::warn!("[MCP] {} 失败: {}", name, e),
            }
        }

        self.tools = all_tools.clone();
        all_tools
    }

    /// 初始化会话并列出工具 (HTTP / stdio 通用)
    async fn init_and_list<S: McpSession>(
        session: &mut S,
        server: &str,
    ) -> Result<Vec<McpToolInfo>, String> {
        session.initialize().await?;
        let raw_tools = session.list_tools().await?;
        let tools: Vec<McpToolInfo> = raw_tools
            .iter()
            .map(|t| {
                let info = Self::parse_tool_info(server, t);
                log::info!("[MCP] 发现工具: {}/{}", server, info.name);
                info
            })
            .collect();
        Ok(tools)
    }

    /// 启动 stdio 服务器 + 初始化 + 收集工具
    async fn spawn_stdio(&mut self, name: &str) -> Result<Vec<McpToolInfo>, String> {
        let (cmd, args, env) = self
            .stdio_configs
            .get(name)
            .ok_or_else(|| format!("{name} 不在 stdio 配置中"))?
            .clone();

        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| format!("{name} 需要 RuntimeManager 但未注入"))?;

        runtime
            .ensure_ready(|_| {})
            .await
            .map_err(|e| format!("{name} 运行时未就绪: {e}"))?;

        let mut session = StdioSession::spawn(runtime, &cmd, &args, &env)
            .await
            .map_err(|e| format!("{name} 启动失败: {e}"))?;

        let tools = Self::init_and_list(&mut session, name).await?;
        self.stdio_sessions.insert(name.to_string(), session);
        Ok(tools)
    }

    /// 调用指定服务器的工具
    pub async fn call(
        &mut self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        if let Some(session) = self.http_sessions.get_mut(server) {
            if session.session_id.is_none() {
                session.initialize().await?;
            }
            return session.call_tool(tool, args).await;
        }

        if let Some(session) = self.stdio_sessions.get_mut(server) {
            return session.call_tool(tool, args).await;
        }

        Err(format!("未知 MCP 服务器: {server}"))
    }

    pub fn server_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .http_sessions
            .keys()
            .chain(self.stdio_configs.keys())
            .chain(self.stdio_sessions.keys())
            .cloned()
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub async fn list_tools_for(&mut self, server: &str) -> Result<Vec<serde_json::Value>, String> {
        if let Some(session) = self.http_sessions.get_mut(server) {
            if session.session_id.is_none() {
                session.initialize().await?;
            }
            return session.list_tools().await;
        }

        if !self.stdio_sessions.contains_key(server) && self.stdio_configs.contains_key(server) {
            self.spawn_stdio(server).await?;
        }
        if let Some(session) = self.stdio_sessions.get_mut(server) {
            return session.list_tools().await;
        }

        Err(format!("未知 MCP 服务器: {server}"))
    }

    pub fn session_status(&self) -> serde_json::Value {
        let servers = self
            .server_names()
            .into_iter()
            .map(|name| {
                let transport = if self.http_sessions.contains_key(&name) {
                    "http"
                } else {
                    "stdio"
                };
                let connected = self
                    .http_sessions
                    .get(&name)
                    .map(|session| session.session_id.is_some())
                    .or_else(|| self.stdio_sessions.get(&name).map(|_| true))
                    .unwrap_or(false);
                serde_json::json!({
                    "server": name,
                    "transport": transport,
                    "connected": connected,
                })
            })
            .collect::<Vec<_>>();

        serde_json::json!({ "servers": servers })
    }

    /// 获取已发现的工具列表
    pub fn tools(&self) -> &[McpToolInfo] {
        &self.tools
    }

    /// 关闭所有 stdio 会话
    pub async fn shutdown(&mut self) {
        self.stdio_sessions.clear();
        log::info!("[MCP] 所有 stdio 会话已关闭");
    }

    /// 解析工具信息
    fn parse_tool_info(server: &str, tool: &serde_json::Value) -> McpToolInfo {
        McpToolInfo {
            server: server.to_string(),
            name: tool["name"].as_str().unwrap_or("").to_string(),
            description: tool["description"].as_str().unwrap_or("").to_string(),
            input_schema: tool
                .get("inputSchema")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default())),
        }
    }
}

/// MCP 会话共同行为 — init_and_list 要求的 trait
trait McpSession {
    async fn initialize(&mut self) -> Result<(), String>;
    async fn list_tools(&mut self) -> Result<Vec<serde_json::Value>, String>;
}

impl McpSession for HttpSession {
    async fn initialize(&mut self) -> Result<(), String> {
        HttpSession::initialize(self).await
    }
    async fn list_tools(&mut self) -> Result<Vec<serde_json::Value>, String> {
        HttpSession::list_tools(self).await
    }
}

impl McpSession for StdioSession {
    async fn initialize(&mut self) -> Result<(), String> {
        StdioSession::initialize(self).await
    }
    async fn list_tools(&mut self) -> Result<Vec<serde_json::Value>, String> {
        StdioSession::list_tools(self).await
    }
}
