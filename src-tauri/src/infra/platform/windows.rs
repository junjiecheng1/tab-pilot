// Windows 平台实现
//
// 所有 pub 函数/常量与 unix.rs 保持相同签名
// 调用方通过 crate::infra::platform 统一访问

use std::path::PathBuf;

// ══════════════════════════════════════════
// 常量
// ══════════════════════════════════════════

/// PATH 环境变量分隔符
pub const PATH_SEP: &str = ";";

/// 可执行文件后缀
pub const EXE_SUFFIX: &str = ".exe";

/// OS 标识 (hello 消息 / 设备上报)
pub const OS_NAME: &str = "windows";

/// OS 显示名
pub const OS_DISPLAY: &str = "Windows";

// ══════════════════════════════════════════
// Shell
// ══════════════════════════════════════════

/// 默认交互式 Shell 路径
pub fn default_shell() -> String {
    // Windows 用 ComSpec (原生约定)，不读 SHELL (Unix 约定)
    std::env::var("ComSpec")
        .unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".to_string())
}

/// 一次性执行命令的前缀 (config.shell_cmd)
pub fn shell_exec_prefix() -> Vec<String> {
    vec!["cmd".into(), "/C".into()]
}

/// Shell 交互模式参数 (-i 仅对 bash/zsh 有意义)
pub fn shell_interactive_args() -> Vec<String> {
    // cmd.exe 不支持 -i
    vec![]
}

/// 命令完成标记: 追加到命令后面用于检测 exit code
pub fn marker_echo(marker: &str) -> String {
    format!("echo {}:%errorlevel%", marker)
}

// ══════════════════════════════════════════
// 路径
// ══════════════════════════════════════════

/// Home 目录
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| {
        PathBuf::from(
            std::env::var("USERPROFILE")
                .unwrap_or_else(|_| "C:\\Users\\Default".into()),
        )
    })
}

/// 临时目录
pub fn temp_dir() -> PathBuf {
    // std::env::temp_dir 在 Windows 上读 TMP/TEMP
    std::env::temp_dir()
}

/// 默认工作目录 fallback 链
pub fn default_workspace() -> String {
    std::env::var("WORKSPACE")
        .or_else(|_| std::env::var("USERPROFILE"))
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
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| {
            std::env::var("TEMP")
                .unwrap_or_else(|_| "C:\\".into())
        })
}

/// 给二进制名加平台后缀: "rg" → "rg.exe"
pub fn bin_name(name: &str) -> String {
    if name.ends_with(".exe") {
        name.to_string()
    } else {
        format!("{name}.exe")
    }
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
    PathBuf::from("node.exe")
}

/// npx 可执行文件相对路径
pub fn npx_bin_relative() -> PathBuf {
    PathBuf::from("npx.cmd")
}

/// npm 可执行文件相对路径
pub fn npm_bin_relative() -> PathBuf {
    PathBuf::from("npm.cmd")
}

/// Node.js 下载包扩展名
pub fn node_archive_ext() -> &'static str {
    "zip"
}

// ══════════════════════════════════════════
// 安全门控
// ══════════════════════════════════════════

/// 系统保护路径
pub fn protected_system_paths() -> Vec<String> {
    vec![
        "C:\\Windows".into(),
        "C:\\Program Files".into(),
    ]
}

// ══════════════════════════════════════════
// PTY 输出清洗
// ══════════════════════════════════════════

/// 是否为 Shell banner 行 (cmd.exe 启动信息)
pub fn is_shell_banner(line: &str) -> bool {
    line.starts_with("Microsoft Windows")
        || line.starts_with("(c) Microsoft Corporation")
        || line.starts_with("(C) Microsoft Corporation")
}

/// 是否为 Shell prompt 行
pub fn is_shell_prompt(line: &str) -> bool {
    // Windows cmd prompt: C:\Users\xxx> 或 D:\workspace>
    line.len() > 2
        && line.as_bytes().get(1) == Some(&b':')
        && line.as_bytes().get(2) == Some(&b'\\')
        && line.ends_with('>')
}

// ══════════════════════════════════════════
// 平台 key (OSS 下载)
// ══════════════════════════════════════════

/// 平台标识 (对应 OSS 目录名)
pub fn platform_key() -> Result<&'static str, String> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("win-x64"),
        arch => Err(format!("不支持的 Windows 架构: {arch}")),
    }
}

// ══════════════════════════════════════════
// 工具 PATH
// ══════════════════════════════════════════

/// Shell PATH 是否应包含 archive 类工具的子目录
/// Windows 不包含: PyInstaller 打包的 markitdown/douyin-cli 含 DLL,
/// 与系统 DLL 冲突导致 cmd.exe 0xc0000142
pub fn should_include_archive_tool_paths() -> bool {
    false
}

// ══════════════════════════════════════════
// Node.js 运行时
// ══════════════════════════════════════════

/// Node.js runtime 搜索目录 (服务器部署约定)
pub fn nodejs_runtime_search_paths() -> Vec<PathBuf> {
    // Windows 桌面端无服务器约定路径
    vec![]
}
