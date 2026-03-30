// TabPilot 配置
//
// .env 只配 host:port，协议自动推导:
//   localhost:8080  → ws://  + http://
//   www.syxc.art    → wss:// + https://

use std::env;
use std::path::PathBuf;

/// Pilot 配置
#[derive(Debug, Clone)]
pub struct PilotConfig {
    // ── 网络 ──────────────────────────
    /// 原始 host:port (如 "localhost:8080")
    pub server_host: String,
    /// WebSocket URL (如 "ws://localhost:8080/ws/pilot")
    pub ws_url: String,
    /// HTTP URL (如 "http://localhost:8080")
    pub http_url: String,

    // ── 路径 ──────────────────────────
    /// 数据目录
    pub data_dir: PathBuf,
    /// 工作目录
    pub workspace: String,

    // ── 安全 ──────────────────────────
    /// 安全模式
    pub guard_mode: String,

    // ── Shell ─────────────────────────
    /// Shell 超时 (秒)
    pub shell_timeout: u64,
    /// Shell 命令前缀
    pub shell_cmd: Vec<String>,

    // ── 限制 ──────────────────────────
    /// 输出最大字节
    pub output_max_size: usize,
    /// 文件最大读取字节
    pub file_max_read_size: usize,

    // ── 环境 ──────────────────────────
    /// 操作系统
    pub os_name: String,
    /// 是否调试模式
    pub debug: bool,

    // ── 外部服务 ──────────────────────
    /// CLI 工具 OSS 下载地址
    pub tools_oss_url: String,
}

impl PilotConfig {
    /// 从环境变量 + 数据目录初始化
    pub fn from_env(data_dir: PathBuf) -> Self {
        if cfg!(debug_assertions) {
            let _ = dotenvy::dotenv();
        }

        let _ = std::fs::create_dir_all(&data_dir);

        let host = env_or_compile("PILOT_SERVER", env!("PILOT_SERVER", "localhost:8080"));
        let (ws_url, http_url) = Self::derive_urls(&host);

        Self {
            server_host: host,
            ws_url,
            http_url,
            data_dir,
            workspace: env_or_default("PILOT_WORKSPACE", &default_workspace()),
            guard_mode: "standard".to_string(),
            shell_timeout: 30,
            shell_cmd: platform_shell_cmd(),
            output_max_size: 50 * 1024,
            file_max_read_size: 100 * 1024,
            os_name: platform_os().to_string(),
            debug: env_bool("PILOT_DEBUG"),
            tools_oss_url: env_or_default(
                "TOOLS_OSS_URL",
                "https://lingostatic.tweet.net.cn/tools/tabpilot-tools",
            ),
        }
    }

    /// 是否是本地开发
    pub fn is_local(&self) -> bool {
        self.server_host.starts_with("localhost")
            || self.server_host.starts_with("127.0.0.1")
    }

    /// 拼接 API URL: /api/pilot/status → http://host/api/pilot/status
    pub fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.http_url, path)
    }

    /// 版本字符串
    pub fn version(&self) -> String {
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x86_64" };
        let os = platform_os_display();
        format!("1.0.0 ({os}_{arch})")
    }

    // ── 内部 ──────────────────────────

    /// 从 host 推导 ws:// / wss:// 和 http:// / https://
    fn derive_urls(host: &str) -> (String, String) {
        let is_local = host.starts_with("localhost") || host.starts_with("127.0.0.1");
        let (ws, http) = if is_local { ("ws", "http") } else { ("wss", "https") };
        (
            format!("{ws}://{host}/ws/pilot"),
            format!("{http}://{host}"),
        )
    }
}

// ── 辅助函数 ──────────────────────────

/// 运行时 env → 编译时 env fallback
fn env_or_compile(key: &str, compile_default: &str) -> String {
    env::var(key).unwrap_or_else(|_| compile_default.to_string())
}

/// 运行时 env → 默认值 fallback
fn env_or_default(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// 布尔环境变量
fn env_bool(key: &str) -> bool {
    env::var(key).map(|v| v == "true" || v == "1").unwrap_or(false)
}

/// 默认工作目录
fn default_workspace() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("tabspace")
        .to_string_lossy()
        .to_string()
}

/// 平台标识 (内部用)
fn platform_os() -> &'static str {
    if cfg!(target_os = "macos") { "darwin" }
    else if cfg!(target_os = "windows") { "windows" }
    else { "linux" }
}

/// 平台显示名
fn platform_os_display() -> &'static str {
    if cfg!(target_os = "macos") { "Darwin" }
    else if cfg!(target_os = "windows") { "Windows" }
    else { "Linux" }
}

/// 平台 Shell 命令前缀
fn platform_shell_cmd() -> Vec<String> {
    if cfg!(target_os = "windows") {
        vec!["cmd".to_string(), "/C".to_string()]
    } else {
        vec!["/bin/bash".to_string(), "-c".to_string()]
    }
}
