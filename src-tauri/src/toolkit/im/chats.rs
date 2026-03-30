// toolkit/im/chats — 群聊操作
//
// 移植自: aily_im/commands/chats.py (281行)

use serde_json::{json, Value};

use crate::toolkit::client::{self, Result, TabClient};

/// 列出群聊（自动分页）
pub async fn list_chats(
    client: &TabClient,
    page_size: i32,
    max_pages: usize,
) -> Result<Vec<Value>> {
    let mut all_chats = Vec::new();
    let mut page_token: Option<String> = None;
    let mut page_count = 0;

    loop {
        let page = client::im::list_chats(
            client,
            page_size,
            page_token.as_deref(),
            None,
        )
        .await?;

        all_chats.extend(page.items);
        page_count += 1;

        if !page.has_more || page_count >= max_pages {
            break;
        }
        page_token = page.page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(all_chats)
}

/// 搜索群聊
pub async fn search_chats(
    client: &TabClient,
    query: &str,
    page_size: i32,
) -> Result<Vec<Value>> {
    let mut all_chats = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let page = client::im::search_chats(
            client,
            query,
            page_size,
            page_token.as_deref(),
        )
        .await?;

        all_chats.extend(page.items);

        if !page.has_more {
            break;
        }
        page_token = page.page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(all_chats)
}

/// 获取群信息
pub async fn get_chat_info(client: &TabClient, chat_id: &str) -> Result<Value> {
    client::im::get_chat_info(client, chat_id).await
}

/// 列出群成员（自动分页）
pub async fn list_chat_members(
    client: &TabClient,
    chat_id: &str,
    page_size: i32,
) -> Result<Vec<Value>> {
    let mut all_members = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let page = client::im::list_chat_members(
            client,
            chat_id,
            page_size,
            page_token.as_deref(),
        )
        .await?;

        all_members.extend(page.items);

        if !page.has_more {
            break;
        }
        page_token = page.page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(all_members)
}

/// 获取与用户的单聊 chat_id
/// 移植自: aily_im/commands/chats.py p2p_chatid
pub async fn p2p_chat_id(
    client: &TabClient,
    user_id: &str,
    id_type: &str,
) -> Result<Value> {
    client::im::p2p_chat_id(client, user_id, id_type).await
}
