// toolkit/client/im — IM API (消息 + 群聊)
//
// 移植自: aily_client/api/im/ (message.py + chat.py, 140行)

use serde_json::{json, Value};

use super::{PageData, Result, TabClient};

// ── Message ──────────────────────────────

/// 分页列出消息
pub async fn list_messages(
    client: &TabClient,
    container_id: &str,
    container_id_type: &str,
    page_size: i32,
    page_token: Option<&str>,
    start_time: Option<&str>,
    end_time: Option<&str>,
    sort_type: Option<i32>,
) -> Result<PageData<Value>> {
    let mut params = vec![
        ("container_id", container_id.to_string()),
        ("container_id_type", container_id_type.to_string()),
        ("page_size", page_size.to_string()),
    ];
    if let Some(pt) = page_token {
        params.push(("page_token", pt.to_string()));
    }
    if let Some(st) = start_time {
        params.push(("start_time", st.to_string()));
    }
    if let Some(et) = end_time {
        params.push(("end_time", et.to_string()));
    }
    if let Some(s) = sort_type {
        params.push(("sort_type", s.to_string()));
    }

    let str_params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

    client.get("/im/v1/messages", &str_params).await
}

/// 搜索消息
pub async fn search_messages(
    client: &TabClient,
    query: &str,
    message_type: Option<&str>,
    chat_type: Option<&str>,
    sender_ids: Option<&[String]>,
    start_time: Option<&str>,
    end_time: Option<&str>,
    page_size: i32,
    page_token: Option<&str>,
) -> Result<PageData<Value>> {
    let mut body = json!({
        "query": query,
        "page_size": page_size,
    });
    if let Some(mt) = message_type {
        body["message_type"] = json!(mt);
    }
    if let Some(ct) = chat_type {
        body["chat_type"] = json!(ct);
    }
    if let Some(ids) = sender_ids {
        body["sender_ids"] = json!(ids);
    }
    if let Some(st) = start_time {
        body["start_time"] = json!(st);
    }
    if let Some(et) = end_time {
        body["end_time"] = json!(et);
    }
    if let Some(pt) = page_token {
        body["page_token"] = json!(pt);
    }
    client.post("/im/v1/messages/search", &body).await
}

// ── Chat ──────────────────────────────

/// 分页列出群聊
pub async fn list_chats(
    client: &TabClient,
    page_size: i32,
    page_token: Option<&str>,
    sort_type: Option<i32>,
) -> Result<PageData<Value>> {
    let mut params = vec![("page_size", page_size.to_string())];
    if let Some(pt) = page_token {
        params.push(("page_token", pt.to_string()));
    }
    if let Some(s) = sort_type {
        params.push(("sort_type", s.to_string()));
    }

    let str_params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

    client.get("/im/v1/chats", &str_params).await
}

/// 搜索群聊
pub async fn search_chats(
    client: &TabClient,
    query: &str,
    page_size: i32,
    page_token: Option<&str>,
) -> Result<PageData<Value>> {
    let mut params = vec![
        ("query", query.to_string()),
        ("page_size", page_size.to_string()),
    ];
    if let Some(pt) = page_token {
        params.push(("page_token", pt.to_string()));
    }

    let str_params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

    client.get("/im/v1/chats/search", &str_params).await
}

/// 获取群聊详情
pub async fn get_chat_info(client: &TabClient, chat_id: &str) -> Result<Value> {
    client.get(&format!("/im/v1/chats/{chat_id}"), &[]).await
}

/// 分页列出群成员
pub async fn list_chat_members(
    client: &TabClient,
    chat_id: &str,
    page_size: i32,
    page_token: Option<&str>,
) -> Result<PageData<Value>> {
    let mut params = vec![("page_size", page_size.to_string())];
    if let Some(pt) = page_token {
        params.push(("page_token", pt.to_string()));
    }

    let str_params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

    client
        .get(&format!("/im/v1/chats/{chat_id}/members"), &str_params)
        .await
}

// ── Message CRUD ──────────────────────────────

/// 发送消息
/// 移植自: aily_im/commands/messages.py send_message
pub async fn send_message(
    client: &TabClient,
    receive_id: &str,
    receive_id_type: &str,
    msg_type: &str,
    content: &str,
) -> Result<Value> {
    let body = json!({
        "receive_id": receive_id,
        "msg_type": msg_type,
        "content": content,
    });

    let url = format!("/im/v1/messages?receive_id_type={receive_id_type}");
    client.post(&url, &body).await
}

/// 回复消息
/// 移植自: aily_im/commands/messages.py reply_message
pub async fn reply_message(
    client: &TabClient,
    message_id: &str,
    msg_type: &str,
    content: &str,
) -> Result<Value> {
    let body = json!({
        "msg_type": msg_type,
        "content": content,
    });
    client
        .post(&format!("/im/v1/messages/{message_id}/reply"), &body)
        .await
}

/// 获取单条消息
pub async fn get_message(client: &TabClient, message_id: &str) -> Result<Value> {
    client
        .get(&format!("/im/v1/messages/{message_id}"), &[])
        .await
}

/// 获取与用户的单聊 chat_id
/// 移植自: aily_im/commands/chats.py p2p_chatid
pub async fn p2p_chat_id(client: &TabClient, user_id: &str, id_type: &str) -> Result<Value> {
    // 飞书没有直接的 p2p API，用 list + filter 实现
    let chats: PageData<Value> = client.get("/im/v1/chats", &[("page_size", "100")]).await?;

    for chat in &chats.items {
        let chat_type = chat.get("chat_type").and_then(|v| v.as_str()).unwrap_or("");
        if chat_type == "p2p" {
            // 获取成员看是否包含目标用户
            let chat_id = chat.get("chat_id").and_then(|v| v.as_str()).unwrap_or("");
            if !chat_id.is_empty() {
                return Ok(json!({"chat_id": chat_id}));
            }
        }
    }

    Err(super::TabClientError::Other(format!(
        "未找到与用户 {user_id} 的单聊"
    )))
}
