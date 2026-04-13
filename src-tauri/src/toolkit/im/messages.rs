// toolkit/im/messages — 消息列出/搜索
//
// 移植自: aily_im/commands/messages.py

use serde_json::{json, Value};

use crate::toolkit::client::{self, Result, TabClient};

/// 列出消息（自动分页）
pub async fn list_messages(
    client: &TabClient,
    container_id: &str,
    container_id_type: &str,
    page_size: i32,
    max_pages: usize,
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> Result<Vec<Value>> {
    let mut all_messages = Vec::new();
    let mut page_token: Option<String> = None;
    let mut page_count = 0;

    loop {
        let page = client::im::list_messages(
            client,
            container_id,
            container_id_type,
            page_size,
            page_token.as_deref(),
            start_time,
            end_time,
            None,
        )
        .await?;

        all_messages.extend(page.items);
        page_count += 1;

        if !page.has_more || page_count >= max_pages {
            break;
        }
        page_token = page.page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(all_messages)
}

/// 搜索消息
pub async fn search_messages(
    client: &TabClient,
    query: &str,
    page_size: i32,
    max_pages: usize,
    message_type: Option<&str>,
    chat_type: Option<&str>,
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> Result<Vec<Value>> {
    let mut all_messages = Vec::new();
    let mut page_token: Option<String> = None;
    let mut page_count = 0;

    loop {
        let page = client::im::search_messages(
            client,
            query,
            message_type,
            chat_type,
            None,
            start_time,
            end_time,
            page_size,
            page_token.as_deref(),
        )
        .await?;

        all_messages.extend(page.items);
        page_count += 1;

        if !page.has_more || page_count >= max_pages {
            break;
        }
        page_token = page.page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(all_messages)
}

/// 发送消息
/// 移植自: aily_im/commands/messages.py send_message
pub async fn send_message(
    client: &TabClient,
    receive_id: &str,
    receive_id_type: &str,
    msg_type: &str,
    content: &str,
) -> Result<Value> {
    client::im::send_message(client, receive_id, receive_id_type, msg_type, content).await
}

/// 回复消息
/// 移植自: aily_im/commands/messages.py reply_message
pub async fn reply_message(
    client: &TabClient,
    message_id: &str,
    msg_type: &str,
    content: &str,
) -> Result<Value> {
    client::im::reply_message(client, message_id, msg_type, content).await
}

/// 获取单条消息
pub async fn get_message(client: &TabClient, message_id: &str) -> Result<Value> {
    client::im::get_message(client, message_id).await
}
