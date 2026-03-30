use std::env::consts;

/// 平台标识 (对应 OSS 目录名)
pub fn platform_key() -> Result<&'static str, String> {
    match (consts::OS, consts::ARCH) {
        ("macos", "aarch64") => Ok("darwin-arm64"),
        ("macos", "x86_64") => Ok("darwin-x64"),
        ("linux", "x86_64") => Ok("linux-x64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("windows", "x86_64") => Ok("win-x64"),
        (os, arch) => Err(format!("不支持的平台: {os}-{arch}")),
    }
}

/// 工具类型
pub enum ToolKind {
    /// 单文件二进制 (rg, fd, jq, yq)
    Binary,
    /// tar.gz 打包目录 (markitdown)
    Archive,
    /// tar.gz 形式下载的单文件二进制 (lark-cli)
    TarGzDirect,
}

/// 工具清单
pub fn tool_list() -> Vec<(&'static str, ToolKind, bool)> {
    vec![
        // ── 通用 CLI (单文件) ────────
        ("rg", ToolKind::Binary, false),
        ("fd", ToolKind::Binary, false),
        ("jq", ToolKind::Binary, false),
        ("yq", ToolKind::Binary, false),
        // ── 打包工具 (目录) ──────────
        ("markitdown", ToolKind::Archive, false),
        ("lark-cli", ToolKind::TarGzDirect, true), // 动态版本探测
    ]
}
