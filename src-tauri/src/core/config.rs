// 应用配置 — 从环境变量/默认值加载
//
// 对应 Python app/core/config.py

use std::path::PathBuf;

/// 应用级配置
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// CORS 允许域
    pub origins: Vec<String>,
    /// 服务超时 (分钟), None 表示不超时
    pub service_timeout_minutes: Option<u32>,
    /// 日志级别
    pub log_level: String,
    /// MCP 服务器配置文件路径
    pub mcp_servers_config: PathBuf,
    /// 浏览器语言
    pub browser_lang: String,
    /// Skills 自动挂载路径
    pub skills_path: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            origins: vec!["*".to_string()],
            service_timeout_minutes: None,
            log_level: "INFO".to_string(),
            mcp_servers_config: PathBuf::from(
                std::env::var("MCP_SERVERS_CONFIG").unwrap_or_else(|_| "mcp-servers.json".into()),
            ),
            browser_lang: std::env::var("BROWSER_LANG").unwrap_or_else(|_| "en-US".into()),
            skills_path: std::env::var("AIO_SKILLS_PATH")
                .ok()
                .map(|s| PathBuf::from(s.trim_matches('"').trim_matches('\''))),
        }
    }
}

impl AppConfig {
    /// 从环境变量加载配置
    pub fn from_env() -> Self {
        let origins = std::env::var("ORIGINS")
            .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|_| vec!["*".to_string()]);

        let service_timeout_minutes = std::env::var("SERVICE_TIMEOUT_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok());

        let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "INFO".to_string());

        Self {
            origins,
            service_timeout_minutes,
            log_level,
            ..Default::default()
        }
    }
}
