// 平台抽象 — 按平台编译选择具体实现
//
// 调用方统一用: use crate::infra::platform;
// 不需要知道底层走的是 windows.rs 还是 unix.rs
//
// 新增平台只需:
//   1. 新建 xxx.rs 实现相同的公开函数
//   2. 在这里加一组 cfg + pub use

#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(target_os = "windows"))]
mod unix;

// ── 统一导出 ──────────────────────────

#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(target_os = "windows"))]
pub use unix::*;

// ── 跨平台常量（所有平台一样） ──────────

/// TabPilot 数据根目录名
pub const DATA_DIR_NAME: &str = ".tabpilot";

/// Tools 子目录
pub const TOOLS_SUBDIR: &str = "runtime/tools";

/// 运行时子目录
pub const RUNTIME_SUBDIR: &str = "runtime";
