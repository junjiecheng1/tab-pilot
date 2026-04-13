// 命令执行 — command 级状态跟踪

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::core::error::{ServiceError, ServiceResult};
use crate::infra::pty_clean::clean_pty_output;

use super::session::{ShellCommandState, ShellSession};

const POLL_INTERVAL_MS: u64 = 100;

pub fn sync_command_state(session: &mut ShellSession) -> Result<(), ServiceError> {
    session.refresh_active();

    let current = match session.current_command.clone() {
        Some(current) => current,
        None => return Ok(()),
    };

    let raw_output = session.collector.take();
    let cleaned_output = sanitize_command_output(&raw_output, &current.command, &current.marker);

    if let Some(exit_code) = parse_marker_exit_code(&raw_output, &current.marker) {
        session
            .complete_current_command(cleaned_output, exit_code)
            .map_err(ServiceError::internal)?;
        return Ok(());
    }

    if !session.active {
        session.interrupt_current_command(cleaned_output);
        return Ok(());
    }

    session
        .set_current_output(cleaned_output)
        .map_err(ServiceError::internal)?;
    Ok(())
}

pub async fn exec_in_session(
    session: Arc<Mutex<ShellSession>>,
    command: &str,
    timeout_secs: Option<u64>,
) -> ServiceResult {
    let timeout_val = timeout_secs.unwrap_or(30);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_val);
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

    let (session_id, command_id) = {
        let mut locked = session.lock().await;
        let command_state = locked
            .begin_command(command)
            .map_err(ServiceError::bad_request)?;
        let wrapped = format!("{command}\n{}", command_wrapper_line(&command_state.marker));
        locked
            .write_command(&wrapped)
            .map_err(ServiceError::internal)?;
        log::info!(
            "[Shell] exec start: sid={}, cmd_id={}, timeout={}s",
            &locked.id[..locked.id.len().min(8)],
            &command_state.id[..command_state.id.len().min(8)],
            timeout_val,
        );
        (locked.id.clone(), command_state.id)
    };

    loop {
        tokio::time::sleep(poll_interval).await;
        let now = tokio::time::Instant::now();

        let maybe_result = {
            let mut locked = session.lock().await;
            sync_command_state(&mut locked)?;
            let (snapshot, latest) = locked
                .snapshot_command(Some(&command_id))
                .map_err(ServiceError::bad_request)?;

            if snapshot.command_done {
                Some(command_payload(
                    &session_id,
                    &snapshot,
                    locked.active,
                    latest,
                ))
            } else if now >= deadline {
                let timeout_snapshot = if locked.current_command_id() == Some(command_id.as_str()) {
                    locked
                        .mark_current_timed_out(snapshot.output.clone())
                        .map_err(ServiceError::internal)?
                } else {
                    snapshot
                };
                Some(command_payload(
                    &session_id,
                    &timeout_snapshot,
                    locked.active,
                    latest,
                ))
            } else {
                None
            }
        };

        if let Some(result) = maybe_result {
            let status = result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let exit_code = result.get("exit_code").and_then(Value::as_i64);
            log::info!(
                "[Shell] exec done: sid={}, cmd_id={}, status={}, exit={:?}",
                &session_id[..session_id.len().min(8)],
                &command_id[..command_id.len().min(8)],
                status,
                exit_code,
            );
            return Ok(result);
        }
    }
}

pub async fn view_output(
    session: Arc<Mutex<ShellSession>>,
    command_id: Option<&str>,
) -> ServiceResult {
    let mut locked = session.lock().await;
    sync_command_state(&mut locked)?;
    let (snapshot, latest) = locked
        .snapshot_command(command_id)
        .map_err(ServiceError::bad_request)?;
    Ok(command_payload(
        &locked.id,
        &snapshot,
        locked.active,
        latest,
    ))
}

pub async fn write_text(
    session: Arc<Mutex<ShellSession>>,
    text: &str,
    command_id: Option<&str>,
    press_enter: bool,
) -> ServiceResult {
    let mut locked = session.lock().await;
    sync_command_state(&mut locked)?;

    let current_id = locked
        .current_command_id()
        .ok_or_else(|| ServiceError::bad_request("当前 session 没有运行中的 command"))?
        .to_string();
    if let Some(expected_id) = command_id {
        if expected_id != current_id {
            return Err(ServiceError::bad_request(format!(
                "当前运行中的 command 不是: {expected_id}"
            )));
        }
    }

    let payload = if press_enter {
        format!("{text}\n")
    } else {
        text.to_string()
    };
    let written = locked.write_raw(&payload).map_err(ServiceError::internal)?;
    sync_command_state(&mut locked)?;

    let (snapshot, latest) = locked
        .snapshot_command(Some(&current_id))
        .map_err(ServiceError::bad_request)?;
    let mut result = command_payload(&locked.id, &snapshot, locked.active, latest);
    result["written"] = json!(written);
    Ok(result)
}

pub async fn wait_session(
    session: Arc<Mutex<ShellSession>>,
    command_id: Option<&str>,
    timeout_secs: u64,
) -> ServiceResult {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

    loop {
        let now = tokio::time::Instant::now();
        let maybe_result = {
            let mut locked = session.lock().await;
            sync_command_state(&mut locked)?;
            let target_id = command_id
                .map(|value| value.to_string())
                .or_else(|| locked.current_command_id().map(|value| value.to_string()));
            let (snapshot, latest) = locked
                .snapshot_command(target_id.as_deref())
                .map_err(ServiceError::bad_request)?;

            if snapshot.command_done {
                Some(command_payload(
                    &locked.id,
                    &snapshot,
                    locked.active,
                    latest,
                ))
            } else if now >= deadline {
                let timeout_snapshot = if locked.current_command_id() == target_id.as_deref() {
                    locked
                        .mark_current_timed_out(snapshot.output.clone())
                        .map_err(ServiceError::internal)?
                } else {
                    snapshot
                };
                Some(command_payload(
                    &locked.id,
                    &timeout_snapshot,
                    locked.active,
                    latest,
                ))
            } else {
                None
            }
        };

        if let Some(result) = maybe_result {
            return Ok(result);
        }
        tokio::time::sleep(poll_interval).await;
    }
}

pub fn command_payload(
    session_id: &str,
    command: &ShellCommandState,
    session_alive: bool,
    latest: bool,
) -> Value {
    json!({
        "session_id": session_id,
        "command_id": command.id,
        "status": command.status,
        "command_done": command.command_done,
        "timed_out": command.timed_out,
        "session_alive": session_alive,
        "active": session_alive,
        "latest": latest,
        "exit_code": command.exit_code,
        "output": command.output,
    })
}

fn parse_marker_exit_code(raw_output: &str, marker: &str) -> Option<i32> {
    let prefix = format!("{marker}:");
    normalize_output(raw_output).lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix(&prefix)
            .and_then(|code| code.trim().parse::<i32>().ok())
    })
}

fn sanitize_command_output(raw_output: &str, command: &str, marker: &str) -> String {
    let wrapper_line = command_wrapper_line(marker);
    let normalized = normalize_output(raw_output);
    let mut filtered = Vec::new();
    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.contains(marker) {
            continue;
        }
        if trimmed == wrapper_line {
            continue;
        }
        filtered.push(line);
    }
    clean_pty_output(&filtered.join("\n"), command)
}

fn normalize_output(raw_output: &str) -> String {
    raw_output.replace('\r', "")
}

fn command_wrapper_line(marker: &str) -> String {
    if cfg!(target_os = "windows") {
        // Windows cmd.exe: echo marker:exitcode
        format!("echo {}:%errorlevel%", marker)
    } else {
        // Unix bash/zsh: printf marker:exitcode
        format!("printf '\\n{}:%s\\n' \"$?\"", marker)
    }
}
