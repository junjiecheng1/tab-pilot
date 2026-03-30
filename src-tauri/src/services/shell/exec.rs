// 命令执行 — 写入 + 轮询等待 + 输出清洗
//
// 使用唯一结束标记 (__DONE__<id>__) 替代 prompt 检测
// 解决输出中包含 $ 等 prompt 相似字符导致的误判问题

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::Mutex;

use crate::core::error::{ServiceError, ServiceResult};
use crate::infra::pty_clean::clean_pty_output;

use super::session::ShellSession;

/// 默认轮询间隔
const POLL_INTERVAL_MS: u64 = 100;
/// 命令结束后额外等待 (让 exit_code 稳定)
const POST_DONE_WAIT_MS: u64 = 50;

/// 在已有会话中执行命令
pub async fn exec_in_session(
    session: Arc<Mutex<ShellSession>>,
    command: &str,
    timeout_secs: Option<u64>,
) -> ServiceResult {
    let timeout_val = timeout_secs.unwrap_or(30);
    log::info!("[Shell] exec: timeout={}s, cmd={}", timeout_val, &command[..command.len().min(120)]);

    // 生成唯一结束标记
    let done_marker = format!("__DONE_{}__", &session.lock().await.id[..8]);

    // 清空收集器 + 写入命令 (附加结束标记)
    {
        let mut s = session.lock().await;
        s.collector.clear();
        // 用 ; 连接结束标记，确保命令完成后输出标记
        let wrapped = format!("{command}; echo {done_marker}");
        s.write_command(&wrapped)
            .map_err(|e| ServiceError::internal(e))?;
    }

    // 轮询等待: 结束标记出现或超时
    let timeout = Duration::from_secs(timeout_val);
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);
    let deadline = tokio::time::Instant::now() + timeout;
    let mut timed_out = false;

    loop {
        tokio::time::sleep(poll_interval).await;

        if tokio::time::Instant::now() >= deadline {
            timed_out = true;
            break;
        }

        // 检查结束标记
        let s = session.lock().await;
        if s.collector.contains_marker(&done_marker) {
            break;
        }
    }

    // 标记检测到后，短暂等待让 exit_code 稳定
    if !timed_out {
        tokio::time::sleep(Duration::from_millis(POST_DONE_WAIT_MS)).await;
    }

    // 读取结果
    let mut s = session.lock().await;
    let raw_output = s.collector.take();
    // 清洗: 去掉 PTY 控制码 + 去掉结束标记行
    let output = clean_pty_output(&raw_output, command);
    let output = remove_done_marker(&output, &done_marker);
    let exit_code = s.try_exit_code();

    log::info!(
        "[Shell] exec done: sid={}, timed_out={}, exit={:?}, output_len={}",
        &s.id[..8], timed_out, exit_code, output.len()
    );

    Ok(json!({
        "session_id": s.id,
        "output": output,
        "exit_code": exit_code,
        "active": s.active,
    }))
}

/// 查看当前输出 (不执行命令)
pub async fn view_output(
    session: Arc<Mutex<ShellSession>>,
) -> ServiceResult {
    let s = session.lock().await;
    let output = s.collector.take();
    Ok(json!({
        "session_id": s.id,
        "output": output,
        "active": s.active,
    }))
}

/// 写入文本
pub async fn write_text(
    session: Arc<Mutex<ShellSession>>,
    text: &str,
) -> ServiceResult {
    let mut s = session.lock().await;
    let written = s.write_raw(text)
        .map_err(|e| ServiceError::internal(e))?;
    Ok(json!({"written": written}))
}

/// 从输出中移除结束标记行
fn remove_done_marker(output: &str, marker: &str) -> String {
    output
        .lines()
        .filter(|line| !line.contains(marker))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 等待会话完成 (active=false 或超时)
pub async fn wait_session(
    session: Arc<Mutex<ShellSession>>,
    timeout_secs: u64,
) -> ServiceResult {
    log::info!("[Shell] wait: timeout={}s", timeout_secs);
    let timeout = Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(500);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        {
            let mut s = session.lock().await;
            if !s.active {
                return Ok(json!({
                    "session_id": s.id,
                    "active": false,
                    "exit_code": s.try_exit_code(),
                    "timed_out": false,
                }));
            }
        }

        if tokio::time::Instant::now() >= deadline {
            let mut s = session.lock().await;
            return Ok(json!({
                "session_id": s.id,
                "active": s.active,
                "exit_code": s.try_exit_code(),
                "timed_out": true,
            }));
        }

        tokio::time::sleep(poll_interval).await;
    }
}
