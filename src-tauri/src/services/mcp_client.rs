// MCP 客户端 — 配置加载 + 工具调用
//
// 对应 Python app/services/mcp_client.py
// 注意: 实际的 MCP Bridge (stdio 子进程管理) 在 pilot/mcp/

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// MCP 服务器配置
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct McpServerConfig {
    /// 连接 URL (streamable-http / sse)
    #[serde(default)]
    pub url: Option<String>,
    /// 启动命令 (stdio)
    #[serde(default)]
    pub command: Option<String>,
    /// 命令参数
    #[serde(default)]
    pub args: Vec<String>,
    /// 环境变量
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// 工作目录
    #[serde(default)]
    pub cwd: Option<String>,
    /// 连接类型: streamable-http / sse / stdio
    #[serde(default = "default_type")]
    pub r#type: String,
    /// 超时 (秒)
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// 工具名前缀
    #[serde(default)]
    pub prefix: Option<String>,
    /// 是否隐藏 (延迟加载)
    #[serde(default)]
    pub hidden: bool,
}

fn default_type() -> String {
    "streamable-http".to_string()
}
fn default_timeout() -> u64 {
    30
}

/// MCP 配置文件根结构
#[derive(Debug, serde::Deserialize)]
struct McpConfigFile {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerConfig>,
}

/// MCP 客户端 — 管理 MCP 服务器配置
pub struct McpClient {
    /// 所有已配置的服务器
    servers: HashMap<String, McpServerConfig>,
    /// 排除列表过滤后的服务器
    filtered_servers: HashMap<String, McpServerConfig>,
}

impl McpClient {
    /// 从配置文件加载
    pub fn load(config_path: &Path) -> Self {
        let servers = Self::load_mcp_servers(config_path);
        let filtered_servers = Self::apply_filter(&servers);
        log::info!(
            "[McpClient] 加载 {} 个服务器 (过滤后 {})",
            servers.len(),
            filtered_servers.len()
        );
        Self {
            servers,
            filtered_servers,
        }
    }

    /// 空客户端
    pub fn empty() -> Self {
        Self {
            servers: HashMap::new(),
            filtered_servers: HashMap::new(),
        }
    }

    /// 从配置文件解析 MCP 服务器列表
    pub fn load_mcp_servers(config_path: &Path) -> HashMap<String, McpServerConfig> {
        if !config_path.exists() {
            log::warn!("[McpClient] 配置文件不存在: {:?}", config_path);
            return HashMap::new();
        }
        match std::fs::read_to_string(config_path) {
            Ok(content) => match serde_json::from_str::<McpConfigFile>(&content) {
                Ok(config) => config.mcp_servers,
                Err(e) => {
                    log::error!("[McpClient] 解析配置失败: {}", e);
                    HashMap::new()
                }
            },
            Err(e) => {
                log::error!("[McpClient] 读取配置失败: {}", e);
                HashMap::new()
            }
        }
    }

    /// 按 MCP_FILTER_SERVERS 环境变量过滤 (黑名单)
    fn apply_filter(
        servers: &HashMap<String, McpServerConfig>,
    ) -> HashMap<String, McpServerConfig> {
        let filter_env = std::env::var("MCP_FILTER_SERVERS").unwrap_or_default();
        if filter_env.is_empty() {
            return servers.clone();
        }
        let excluded: Vec<&str> = filter_env.split(',').map(|s| s.trim()).collect();
        servers
            .iter()
            .filter(|(name, _)| {
                if excluded.contains(&name.as_str()) {
                    log::info!("[McpClient] 排除服务器: {}", name);
                    false
                } else {
                    true
                }
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// 获取过滤后的服务器配置
    pub fn servers(&self) -> &HashMap<String, McpServerConfig> {
        &self.filtered_servers
    }

    /// 获取所有服务器配置 (含被过滤的)
    pub fn all_servers(&self) -> &HashMap<String, McpServerConfig> {
        &self.servers
    }

    /// 获取指定服务器配置
    pub fn get_server(&self, name: &str) -> Option<&McpServerConfig> {
        self.filtered_servers.get(name)
    }

    /// 获取非隐藏服务器
    pub fn visible_servers(&self) -> HashMap<String, &McpServerConfig> {
        self.filtered_servers
            .iter()
            .filter(|(_, cfg)| !cfg.hidden)
            .map(|(k, v)| (k.clone(), v))
            .collect()
    }

    /// 获取隐藏服务器
    pub fn hidden_servers(&self) -> HashMap<String, &McpServerConfig> {
        self.filtered_servers
            .iter()
            .filter(|(_, cfg)| cfg.hidden)
            .map(|(k, v)| (k.clone(), v))
            .collect()
    }

    /// 转为 JSON (用于 WS hello 等场景)
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.filtered_servers).unwrap_or(serde_json::json!({}))
    }

    /// 获取配置文件路径
    pub fn config_path_from_env() -> PathBuf {
        PathBuf::from(
            std::env::var("MCP_SERVERS_CONFIG").unwrap_or_else(|_| "mcp-servers.json".into()),
        )
    }
}
