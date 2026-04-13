// Deep Link 处理
//
// tabpilot://auth?token=xxx&challenge=yyy
// 收到后: 验证 challenge → 保存 token → 唤醒连接 → 通知后端

use crate::router::AppState;
use tauri::{AppHandle, Emitter, Manager};

/// 从 deep link URL 提取 token 和 challenge
fn extract_auth_from_url(payload: &str) -> Option<(String, String)> {
    let url_str = payload
        .trim_matches(|c| c == '[' || c == ']' || c == '"')
        .to_string();

    if let Ok(url) = url::Url::parse(&url_str) {
        let mut token = String::new();
        let mut challenge = String::new();
        for (key, value) in url.query_pairs() {
            if key == "token" {
                token = value.to_string();
            }
            if key == "challenge" {
                challenge = value.to_string();
            }
        }
        if !token.is_empty() {
            return Some((token, challenge));
        }
    }
    None
}

/// 处理 deep link 事件
pub async fn handle_deep_link(handle: AppHandle, payload: &str) {
    log::info!("[DeepLink] 收到: {}", payload);

    let (token, challenge) = match extract_auth_from_url(payload) {
        Some(v) => v,
        None => return,
    };

    let state = handle.state::<AppState>();

    // Challenge 验证 (best-effort: dev/release 进程可能不同)
    let mut auth = state.auth.write().await;
    let verified = auth.verify_challenge(&challenge);
    if !verified && !challenge.is_empty() {
        log::warn!("[DeepLink] challenge 不匹配, 仍接受 token");
    }

    // 保存 token + 唤醒连接
    auth.save_token(token.clone());
    drop(auth);
    state.connector.wake();
    let _ = handle.emit("pilot-auth-success", &token);
    log::info!("[DeepLink] 授权成功, token 已保存");

    // 通知后端 challenge 已确认
    let confirm_url = state.config.api_url("/api/pilot/auth-confirm");
    let client = reqwest::Client::new();
    match client
        .post(&confirm_url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"challenge": challenge}))
        .send()
        .await
    {
        Ok(_) => log::info!("[DeepLink] challenge 已通知后端"),
        Err(e) => log::warn!("[DeepLink] 通知后端失败: {}", e),
    }

    // 显示主窗口
    if let Some(w) = handle.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
