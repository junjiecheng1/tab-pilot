// MCP 配置结构体

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCP 服务器传输配置
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub(super) enum McpTransport {
    Http {
        url: String,
    },
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
}

/// MCP 全局配置
#[derive(Debug, Clone, Deserialize)]
pub(super) struct McpConfig {
    pub servers: HashMap<String, McpTransport>,
}

/// MCP 工具描述 (对外暴露)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub server: String,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
