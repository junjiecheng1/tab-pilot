// toolkit/doc/comments — 文档评论操作
//
// 移植自: aily_doc/commands/comments.py (251行)

use serde_json::{json, Value};
use crate::toolkit::client::{self, Result, TabClient};

/// 获取文档评论（含递归用户名解析）
pub async fn get_comments(
    client: &TabClient,
    file_token: &str,
    file_type: &str,
    resolve_users: bool,
) -> Result<Value> {
    let raw = client::doc::get_comments(client, file_token, file_type).await?;

    if !resolve_users {
        return Ok(raw);
    }

    // 收集所有 user_id
    let mut user_ids: Vec<String> = Vec::new();
    if let Some(items) = raw.get("items").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(uid) = item.get("user_id").and_then(|v| v.as_str()) {
                if !user_ids.contains(&uid.to_string()) {
                    user_ids.push(uid.to_string());
                }
            }
            // 子评论
            if let Some(replies) = item.get("reply_list").and_then(|v| v.get("replies")).and_then(|v| v.as_array()) {
                for reply in replies {
                    if let Some(uid) = reply.get("user_id").and_then(|v| v.as_str()) {
                        if !user_ids.contains(&uid.to_string()) {
                            user_ids.push(uid.to_string());
                        }
                    }
                }
            }
        }
    }

    // 批量获取用户名
    let mut user_names = std::collections::HashMap::new();
    if !user_ids.is_empty() {
        if let Ok(users_data) = client::user::batch_get_users(client, &user_ids, "user_id").await {
            if let Some(items) = users_data.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    let uid = item.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or(uid);
                    user_names.insert(uid.to_string(), name.to_string());
                }
            }
        }
    }

    // 注入用户名
    let mut result = raw.clone();
    if let Some(items) = result.get_mut("items").and_then(|v| v.as_array_mut()) {
        for item in items {
            inject_user_name(item, &user_names);
        }
    }

    Ok(result)
}

fn inject_user_name(item: &mut Value, user_names: &std::collections::HashMap<String, String>) {
    if let Some(uid) = item.get("user_id").and_then(|v| v.as_str()).map(|s| s.to_string()) {
        if let Some(name) = user_names.get(&uid) {
            item["user_name"] = json!(name);
        }
    }
    if let Some(replies) = item.get_mut("reply_list").and_then(|v| v.get_mut("replies")).and_then(|v| v.as_array_mut()) {
        for reply in replies {
            inject_user_name(reply, user_names);
        }
    }
}

/// 添加评论
pub async fn add_comment(
    client: &TabClient,
    file_token: &str,
    file_type: &str,
    content: &str,
) -> Result<Value> {
    client::doc::add_comment(client, file_token, file_type, content).await
}
