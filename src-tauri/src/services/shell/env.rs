// Shell 命令环境变量注入 — 被 session.rs (persistent PTY) 和 oneshot.rs
// (spawn-per-command) 共用, 避免两处逻辑漂移.
//
// 目前唯一职责是 PATH 组装, 如果后面还有其他跨两条通道的 env 规则, 也应集中到这里.

use std::collections::HashMap;

/// 计算 shell 命令应使用的 PATH.
///
/// 组装规则 (从前到后的优先级):
///   1. tabpilot 的 tools_dir (~/.tabpilot/runtime/tools) — 必放最前,
///      让 `lark-cli` / `rg` / `jq` 等单文件工具能被 `which` / 系统 shell 找到
///   2. (仅 Unix) archive 子目录 (markitdown / douyin-cli 等 PyInstaller 打包目录)
///   3. 用户传入 env 的 PATH (优先) 或当前进程的 PATH
///
/// 平台差异:
///   - Unix: tools_dir 根 + archive 子目录全部注入
///   - Windows: 仅 tools_dir 根. 排除 archive 子目录是必须的 — 里面的 PyInstaller
///     bundled DLL 会被 cmd.exe 启动阶段的 KnownDLL/AppCompat LoadLibrary 加载,
///     与系统 DLL 冲突触发 0xc0000142 (STATUS_DLL_INIT_FAILED). tools_dir 根
///     下是 rg.exe / lark-cli.exe 等单文件, 无 DLL, 不存在此风险.
///   - tools_dir 不存在 (首次启动, 尚未下载完): 返回 None, 不强行塞空路径.
///
/// 返回值:
///   - Some(path_string): 调用方应 `cmd.env("PATH", path_string)` — 这会覆盖
///     user_env 里的 PATH, 但组装时已经把它 append 进去了, 所以信息无损失
///   - None: 无需注入, 调用方保留原本行为即可 (portable_pty 显式继承父进程,
///     tokio::process::Command 默认继承父进程)
pub fn computed_path(user_env: Option<&HashMap<String, String>>) -> Option<String> {
    let tools_mgr = crate::infra::tools::ToolsManager::default();
    let tools_dir = tools_mgr.tools_dir().to_path_buf();
    if !tools_dir.exists() {
        return None;
    }

    // Unix: 加上 archive 子目录 (PyInstaller 打包工具)
    // Windows: 只加 tools_dir 根 (避开 DLL 冲突, 同时让单文件工具如 lark-cli.exe 可见)
    let prefix_dirs: Vec<String> = if crate::infra::platform::should_include_archive_tool_paths() {
        tools_mgr
            .path_dirs()
            .into_iter()
            .map(|d| d.display().to_string())
            .collect()
    } else {
        vec![tools_dir.display().to_string()]
    };

    // base path 优先级: 用户传入 env 的 PATH > 当前进程 PATH
    let base_path = user_env
        .and_then(|e| e.get("PATH"))
        .cloned()
        .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());

    let sep = crate::infra::platform::PATH_SEP;
    let mut parts = prefix_dirs;
    if !base_path.is_empty() {
        parts.push(base_path);
    }
    Some(parts.join(sep))
}
