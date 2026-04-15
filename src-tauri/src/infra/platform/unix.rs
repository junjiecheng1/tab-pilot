// Unix 平台实现 (macOS + Linux)
//
// 所有 pub 函数/常量与 windows.rs 保持相同签名
// 调用方通过 crate::infra::platform 统一访问

use std::path::PathBuf;

// ══════════════════════════════════════════
// 常量
// ══════════════════════════════════════════

/// PATH 环境变量分隔符
pub const PATH_SEP: &str = ":";

/// 可执行文件后缀
pub const EXE_SUFFIX: &str = "";

/// OS 标识 (hello 消息 / 设备上报)
pub const OS_NAME: &str = if cfg!(target_os = "macos") {
    "darwin"
} else {
    "linux"
};

/// OS 显示名
pub const OS_DISPLAY: &str = if cfg!(target_os = "macos") {
    "Darwin"
} else {
    "Linux"
};

// ══════════════════════════════════════════
// Shell
// ══════════════════════════════════════════

/// 默认交互式 Shell 路径
pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

/// 一次性执行命令的前缀 (config.shell_cmd)
pub fn shell_exec_prefix() -> Vec<String> {
    vec!["/bin/bash".into(), "-c".into()]
}

/// Oneshot 执行规格: (program, args_prefix) —— 用户命令作为最后一个参数追加
///
/// Unix 用 bash -c, 不走 PTY
pub fn oneshot_shell_spec() -> (String, Vec<String>) {
    (
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
        vec!["-c".into()],
    )
}

/// Oneshot 命令包装: Unix 无需特殊处理
pub fn wrap_oneshot_command(user_cmd: &str) -> String {
    user_cmd.to_string()
}

/// Shell 交互模式参数
pub fn shell_interactive_args() -> Vec<String> {
    vec!["-i".into()]
}

/// 命令完成标记: 追加到命令后面用于检测 exit code
pub fn marker_echo(marker: &str) -> String {
    format!("printf '\\n{}:%s\\n' \"$?\"", marker)
}

// ══════════════════════════════════════════
// 路径
// ══════════════════════════════════════════

/// Home 目录
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// 临时目录
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

/// 默认工作目录 fallback 链
pub fn default_workspace() -> String {
    std::env::var("WORKSPACE")
        .or_else(|_| std::env::var("HOME"))
        .map(|base| {
            let p = PathBuf::from(&base).join("tabspace");
            p.to_string_lossy().to_string()
        })
        .unwrap_or_else(|_| {
            home_dir()
                .join("tabspace")
                .to_string_lossy()
                .to_string()
        })
}

/// Shell 会话工作目录 fallback
pub fn shell_default_cwd() -> String {
    std::env::var("WORKSPACE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| "/tmp".into())
}

/// 给二进制名加平台后缀: Unix 下不加后缀
pub fn bin_name(name: &str) -> String {
    name.to_string()
}

// ══════════════════════════════════════════
// PATH 操作
// ══════════════════════════════════════════

/// 拼接 PATH 字符串
pub fn join_path(parts: &[&str]) -> String {
    parts.join(PATH_SEP)
}

/// 在系统 PATH 前面追加目录
pub fn prepend_path(dirs: &[&str]) -> String {
    let system_path = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<&str> = dirs.to_vec();
    parts.push(&system_path);
    parts.join(PATH_SEP)
}

// ══════════════════════════════════════════
// Node.js 运行时
// ══════════════════════════════════════════

/// Node 可执行文件相对路径 (相对于 runtime/node/)
pub fn node_bin_relative() -> PathBuf {
    PathBuf::from("bin/node")
}

/// npx 可执行文件相对路径
pub fn npx_bin_relative() -> PathBuf {
    PathBuf::from("bin/npx")
}

/// npm 可执行文件相对路径
pub fn npm_bin_relative() -> PathBuf {
    PathBuf::from("bin/npm")
}

/// Node.js 下载包扩展名
pub fn node_archive_ext() -> &'static str {
    "tar.gz"
}

// ══════════════════════════════════════════
// 安全门控
// ══════════════════════════════════════════

/// 系统保护路径
pub fn protected_system_paths() -> Vec<String> {
    if cfg!(target_os = "macos") {
        vec![
            "/etc".into(),
            "/System".into(),
            "/Library".into(),
        ]
    } else {
        vec![
            "/etc".into(),
            "/root".into(),
            "/boot".into(),
        ]
    }
}

// ══════════════════════════════════════════
// PTY 输出清洗
// ══════════════════════════════════════════

/// 是否为 Shell banner 行 (macOS bash 迁移提示)
pub fn is_shell_banner(line: &str) -> bool {
    line.starts_with("The default interactive shell is now")
        || line.starts_with("To update your account to use zsh")
        || line.starts_with("For more details, please visit https://support.apple.com")
}

/// 是否为 Shell prompt 行
pub fn is_shell_prompt(line: &str) -> bool {
    // bash prompt: bash-3.2$ cmd | bash$ cmd
    if line.starts_with("bash") && line.contains("$ ") {
        return true;
    }
    // 纯 prompt
    if line == "bash-3.2$" || line == "$" {
        return true;
    }
    // zsh prompt: 以 % 结尾且短
    if line.ends_with('%') && line.len() < 80 {
        return true;
    }
    false
}

// ══════════════════════════════════════════
// 平台 key (OSS 下载)
// ══════════════════════════════════════════

/// 平台标识 (对应 OSS 目录名)
pub fn platform_key() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("darwin-arm64"),
        ("macos", "x86_64") => Ok("darwin-x64"),
        ("linux", "x86_64") => Ok("linux-x64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        (os, arch) => Err(format!("不支持的平台: {os}-{arch}")),
    }
}

// ══════════════════════════════════════════
// 工具 PATH
// ══════════════════════════════════════════

/// Shell PATH 是否应包含 archive 类工具的子目录
/// Unix 包含: 没有 DLL 冲突问题
pub fn should_include_archive_tool_paths() -> bool {
    true
}

// ══════════════════════════════════════════
// Node.js 运行时
// ══════════════════════════════════════════

/// Node.js runtime 搜索目录 (服务器部署约定)
pub fn nodejs_runtime_search_paths() -> Vec<PathBuf> {
    vec![PathBuf::from("/opt/runtime/nodejs")]
}
