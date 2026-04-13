// Tauri Commands — 前端 invoke() 直连内存

use crate::infra::tools::{tool_list, ToolsManager};
use crate::router::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct StatusResponse {
    pub running: bool,
    pub connected: bool,
    pub ws_state: String,
    pub server_reachable: bool,
    pub uptime: f64,
    pub guard_mode: String,
    pub workspace: String,
    pub server_url: String,
    pub version: String,
    pub browser_enabled: bool,
    pub audit_enabled: bool,
    pub user_id: String,
    pub user_display: String,
    pub tools_ready: bool,
    pub tool_names: Vec<String>,
}

#[derive(Serialize)]
pub struct LogEntry {
    pub id: i64,
    pub timestamp: f64,
    pub tool_type: String,
    pub action: String,
    pub result: String,
    pub exit_code: i32,
    pub duration: f64,
    pub guard_decision: String,
}

/// 获取状态 — 直接读内存
#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<StatusResponse, String> {
    log::info!("[Cmd] get_status 被调用");
    let conn_state = state.connector.state().await;
    let reachable = state.connector.server_reachable().await;
    let auth = state.auth.read().await;
    let guard = state.guard.read().await;

    // 所有可变设置从 store 读 (单一源), config 做 fallback
    let workspace = state
        .store
        .get_str("settings", "workspace")
        .unwrap_or_else(|| state.config.workspace.clone());
    let browser_enabled = state
        .store
        .get_str("settings", "browser_enabled")
        .map(|v| v == "true")
        .unwrap_or(true);
    let audit_enabled = state
        .store
        .get_str("settings", "audit_enabled")
        .map(|v| v == "true")
        .unwrap_or(true);
    let tools_mgr = ToolsManager::new(
        &std::path::PathBuf::from("/unused"),
        &state.config.tools_oss_url,
    );
    let tool_names = tool_list()
        .into_iter()
        .map(|(name, _, _)| name.to_string())
        .collect();

    Ok(StatusResponse {
        running: true,
        connected: conn_state == crate::router::connector::ConnState::Connected,
        ws_state: conn_state.to_string(),
        server_reachable: reachable,
        uptime: (state.connector.uptime() * 10.0).round() / 10.0,
        guard_mode: guard.mode().to_string(),
        workspace,
        server_url: state.config.ws_url.clone(),
        version: state.config.version(),
        browser_enabled,
        audit_enabled,
        user_id: auth.user_id.clone(),
        user_display: auth.user_display.clone(),
        tools_ready: tools_mgr.is_ready(),
        tool_names,
    })
}

/// 获取审计日志
#[tauri::command]
pub async fn get_logs(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<serde_json::Value>, String> {
    Ok(state.audit.query(limit.unwrap_or(50)).await)
}

/// 退出登录 — 3 行搞定
#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<String, String> {
    state.auth.write().await.clear_token();
    state.connector.disconnect().await;
    Ok("已登出，连接已断开".to_string())
}

/// 保存 Token (OAuth 回调)
#[tauri::command]
pub async fn save_token(
    state: State<'_, AppState>,
    token: String,
    challenge: String,
) -> Result<String, String> {
    // 验证 challenge
    let verified = state.auth.write().await.verify_challenge(&challenge);
    if !verified {
        return Err("challenge 验证失败".to_string());
    }

    state.auth.write().await.save_token(token);
    state.connector.wake(); // 唤醒重连
    Ok("Token 已保存".to_string())
}

/// 获取 auth challenge
#[tauri::command]
pub async fn get_auth_challenge(state: State<'_, AppState>) -> Result<String, String> {
    let challenge = state.auth.write().await.set_challenge();
    Ok(challenge)
}

/// 启动 auth-poll 轮询 (后台任务)
#[tauri::command]
pub async fn start_auth_poll(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    challenge: String,
) -> Result<String, String> {
    let api_base = state.config.http_url.clone();
    let auth = state.auth.clone();
    let connector = state.connector.clone();

    tokio::spawn(crate::ui::auth::poll_for_token(
        challenge, api_base, auth, connector, Some(app),
    ));

    Ok("轮询已启动".to_string())
}

/// 设置安全模式
#[tauri::command]
pub async fn set_guard_mode(state: State<'_, AppState>, mode: String) -> Result<String, String> {
    state.guard.write().await.set_mode(&mode);
    state
        .store
        .set("settings", "guard_mode", serde_json::json!(mode));
    Ok("ok".to_string())
}

/// 设置工作目录
#[tauri::command]
pub async fn set_workspace(state: State<'_, AppState>, path: String) -> Result<String, String> {
    state
        .store
        .set("settings", "workspace", serde_json::json!(path));
    Ok("ok".to_string())
}

/// 设置浏览器开关
#[tauri::command]
pub async fn set_browser_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<String, String> {
    state.store.set(
        "settings",
        "browser_enabled",
        serde_json::json!(enabled.to_string()),
    );
    Ok("ok".to_string())
}

/// 设置审计开关
#[tauri::command]
pub async fn set_audit_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<String, String> {
    state.store.set(
        "settings",
        "audit_enabled",
        serde_json::json!(enabled.to_string()),
    );
    Ok("ok".to_string())
}

/// 获取已记住的命令
#[tauri::command]
pub async fn get_remembered(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.guard.read().await.get_remembered())
}

/// 删除已记住的命令
#[tauri::command]
pub async fn remove_remembered(
    state: State<'_, AppState>,
    prefix: String,
) -> Result<String, String> {
    state.guard.write().await.remove_remembered(&prefix).await;
    Ok("已删除".to_string())
}

/// 清空已记住的命令
#[tauri::command]
pub async fn clear_guard(state: State<'_, AppState>) -> Result<String, String> {
    state.guard.write().await.clear_remembered().await;
    Ok("已清空".to_string())
}

/// 获取保护路径
#[tauri::command]
pub async fn get_protected_paths(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.guard.read().await.protected_paths().to_vec())
}

/// 获取托盘图标可见性
#[tauri::command]
pub async fn get_tray_visible(state: State<'_, AppState>) -> Result<bool, String> {
    let visible = state
        .store
        .get_str("settings", "tray_visible")
        .map(|v| v != "false")
        .unwrap_or(true); // 默认开启
    Ok(visible)
}

/// 设置托盘图标可见性
#[tauri::command]
pub async fn set_tray_visible(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    visible: bool,
) -> Result<String, String> {
    state.store.set(
        "settings",
        "tray_visible",
        serde_json::json!(visible.to_string()),
    );

    // 操作托盘图标
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_visible(visible);
    }
    log::info!("[Cmd] 托盘图标: {}", if visible { "显示" } else { "隐藏" });
    Ok("ok".to_string())
}

/// 浏览器操作 — BrowserView 工具栏调用
#[tauri::command]
pub async fn browser_action(
    state: State<'_, AppState>,
    action: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({});
    state
        .app
        .browser
        .handle(&action, params)
        .await
        .map_err(|e| e.to_string())
}

// ── Shell 终端 ──

/// 列出活跃的 shell 会话
#[tauri::command]
pub async fn list_shell_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    Ok(state.app.get_shell_sessions().await)
}

/// 读取 shell 会话输出 (增量)
#[tauri::command]
pub async fn read_shell_output(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    let result = state
        .app
        .shell
        .view_session(&session_id, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result
        .get("output")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

/// 终止 shell 会话
#[tauri::command]
pub async fn kill_shell_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    state
        .app
        .shell
        .kill_session(&session_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok("已终止".to_string())
}

/// 手动执行 shell 命令 (测试用)
#[tauri::command]
pub async fn exec_shell_command(
    state: State<'_, AppState>,
    command: String,
    timeout: Option<u64>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "command": command,
        "timeout": timeout.unwrap_or(30),
    });
    state
        .app
        .shell
        .handle("exec", params)
        .await
        .map_err(|e| e.to_string())
}
