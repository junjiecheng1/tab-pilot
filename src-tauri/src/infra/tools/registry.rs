// 工具注册表

/// 平台标识 (对应 OSS 目录名) — 委托给 platform 模块
pub fn platform_key() -> Result<&'static str, String> {
    crate::infra::platform::platform_key()
}

/// 工具类型
pub enum ToolKind {
    /// 单文件二进制 (rg, fd, jq, yq)
    Binary,
    /// tar.gz 打包目录 (markitdown, douyin-cli)
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
        ("douyin-cli", ToolKind::Archive, true), // 动态版本探测
        ("lark-cli", ToolKind::TarGzDirect, true), // 动态版本探测
    ]
}
