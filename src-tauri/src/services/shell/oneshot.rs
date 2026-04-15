// Oneshot 命令执行 — 一条命令一个进程, 不走 PTY
//
// 设计理由:
// - 持久 PTY (portable-pty/ConPTY) 在 Windows 上极不稳定 (句柄生命周期、
//   ANSI 转义、行宽自动换行、cmd.exe 延迟展开), 即使修好也脆弱。
// - 大多数命令 (echo / ls / grep / cat) 不需要交互, 一次性执行更简单可靠。
// - 参考 Claude Code (src/utils/Shell.ts) 的 spawn-per-command 模式。
//
// 职责:
// - 用 tokio::process 起子进程, 管道收 stdout/stderr
// - 超时用 start_kill + wait 回收, 避免孤儿进程
// - 输出用 from_utf8_lossy 宽容解码 (Windows 已通过 UTF8 OutputEncoding 前缀保证)

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::core::error::{ServiceError, ServiceResult};

/// 默认超时 (秒) — 与 exec_in_session 保持一致
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// 执行一条命令, 等待完成或超时
///
/// 返回 payload 结构与 persistent 路径 (exec_in_session) 对齐:
/// ```json
/// {
///   "session_id": "",
///   "command_id": "<uuid>",
///   "status": "completed" | "failed" | "timed_out",
///   "command_done": true,
///   "timed_out": bool,
///   "session_alive": false,
///   "active": false,
///   "latest": true,
///   "exit_code": Option<i32>,
///   "output": "<stdout+stderr 合并>"
/// }
/// ```
pub async fn exec_oneshot(
    command: &str,
    cwd: Option<&Path>,
    environment: Option<&HashMap<String, String>>,
    timeout_secs: Option<u64>,
) -> ServiceResult {
    let command_id = uuid::Uuid::new_v4().to_string();
    let timeout_val = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let (program, args_prefix) = crate::infra::platform::oneshot_shell_spec();
    let wrapped = crate::infra::platform::wrap_oneshot_command(command);

    log::info!(
        "[Shell] oneshot start: cmd_id={}, program={}, cwd={:?}, timeout={}s",
        &command_id[..command_id.len().min(8)],
        program,
        cwd,
        timeout_val,
    );

    let mut cmd = Command::new(&program);
    for arg in &args_prefix {
        cmd.arg(arg);
    }
    cmd.arg(&wrapped);

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    if let Some(env) = environment {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // Windows: 阻止黑窗闪现 (CREATE_NO_WINDOW)
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| {
        log::error!("[Shell] oneshot spawn 失败: cmd_id={}, err={e}", &command_id[..8]);
        ServiceError::internal(format!("spawn 失败: {e}"))
    })?;

    // stdout / stderr 并发读取, 避免管道缓冲区填满导致子进程阻塞
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut s) = stdout {
            let _ = s.read_to_end(&mut buf).await;
        }
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_end(&mut buf).await;
        }
        buf
    });

    // 等待进程结束或超时
    let wait_result =
        tokio::time::timeout(Duration::from_secs(timeout_val), child.wait()).await;

    let (exit_code, timed_out) = match wait_result {
        Ok(Ok(status)) => (status.code(), false),
        Ok(Err(e)) => {
            log::warn!("[Shell] oneshot wait 失败: {e}");
            (None, false)
        }
        Err(_) => {
            // 超时: 主动 kill + 等回收
            log::warn!(
                "[Shell] oneshot timeout, killing: cmd_id={}, timeout={}s",
                &command_id[..8],
                timeout_val
            );
            let _ = child.start_kill();
            let _ = child.wait().await;
            (None, true)
        }
    };

    // 收输出 (即使超时也尽量拿到部分输出)
    let stdout_bytes = stdout_task.await.unwrap_or_default();
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    let output = merge_output(&stdout_bytes, &stderr_bytes);

    let status = if timed_out {
        "timed_out"
    } else {
        match exit_code {
            Some(0) => "completed",
            _ => "failed",
        }
    };

    log::info!(
        "[Shell] oneshot done: cmd_id={}, status={}, exit={:?}, output_len={}",
        &command_id[..8],
        status,
        exit_code,
        output.len()
    );

    Ok(json!({
        "session_id": "",
        "command_id": command_id,
        "status": status,
        "command_done": true,
        "timed_out": timed_out,
        "session_alive": false,
        "active": false,
        "latest": true,
        "exit_code": exit_code,
        "output": output,
    }))
}

/// 合并 stdout + stderr, 用 from_utf8_lossy 宽容解码
fn merge_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut s = String::from_utf8_lossy(stdout).into_owned();
    if !stderr.is_empty() {
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&String::from_utf8_lossy(stderr));
    }
    s
}
