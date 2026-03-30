// toolkit/im/dispatch — 消息操作 CLI 命令分发

use serde_json::{json, Value};
use crate::core::error::{ServiceError, ServiceResult};
use crate::toolkit::client::TabClient;

pub async fn dispatch(args: &[String], client: &TabClient) -> ServiceResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match subcmd {
        "list-messages" => {
            let chat_id = named_arg(args, "--chat")?;
            let id_type = named_arg(args, "--id-type").unwrap_or_else(|_| "chat_id".to_string());
            let page_size: i32 = parse_int(args, "--limit", 20);
            let max_pages: usize = parse_int(args, "--max-pages", 1) as usize;
            let start_time = named_arg(args, "--start").ok();
            let end_time = named_arg(args, "--end").ok();

            let result = super::messages::list_messages(
                client, &chat_id, &id_type, page_size, max_pages,
                start_time.as_deref(), end_time.as_deref(),
            ).await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(json!({"messages": result, "count": result.len()}))
        }
        "search-messages" => {
            let query = named_arg(args, "--query")?;
            let page_size: i32 = parse_int(args, "--limit", 20);
            let max_pages: usize = parse_int(args, "--max-pages", 1) as usize;
            let msg_type = named_arg(args, "--type").ok();
            let chat_type = named_arg(args, "--chat-type").ok();
            let start_time = named_arg(args, "--start").ok();
            let end_time = named_arg(args, "--end").ok();

            let result = super::messages::search_messages(
                client, &query, page_size, max_pages,
                msg_type.as_deref(), chat_type.as_deref(),
                start_time.as_deref(), end_time.as_deref(),
            ).await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(json!({"messages": result, "count": result.len()}))
        }
        "list-chats" => {
            let page_size: i32 = parse_int(args, "--limit", 20);
            let max_pages: usize = parse_int(args, "--max-pages", 3) as usize;

            let result = super::chats::list_chats(client, page_size, max_pages)
                .await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(json!({"chats": result, "count": result.len()}))
        }
        "search-chats" => {
            let query = named_arg(args, "--query")?;
            let page_size: i32 = parse_int(args, "--limit", 20);

            let result = super::chats::search_chats(client, &query, page_size)
                .await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(json!({"chats": result, "count": result.len()}))
        }
        "chat-info" => {
            let chat_id = named_arg(args, "--chat")?;
            let result = super::chats::get_chat_info(client, &chat_id)
                .await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "chat-members" => {
            let chat_id = named_arg(args, "--chat")?;
            let page_size: i32 = parse_int(args, "--limit", 100);

            let result = super::chats::list_chat_members(client, &chat_id, page_size)
                .await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(json!({"members": result, "count": result.len()}))
        }
        "send-message" => {
            let receive_id = named_arg(args, "--to")?;
            let id_type = named_arg(args, "--id-type").unwrap_or_else(|_| "chat_id".to_string());
            let msg_type = named_arg(args, "--type").unwrap_or_else(|_| "text".to_string());
            let content = named_arg(args, "--content")?;

            let result = super::messages::send_message(
                client, &receive_id, &id_type, &msg_type, &content,
            ).await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "reply-message" => {
            let message_id = named_arg(args, "--message-id")?;
            let msg_type = named_arg(args, "--type").unwrap_or_else(|_| "text".to_string());
            let content = named_arg(args, "--content")?;

            let result = super::messages::reply_message(
                client, &message_id, &msg_type, &content,
            ).await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "p2p-chatid" => {
            let user_id = named_arg(args, "--user")?;
            let id_type = named_arg(args, "--id-type").unwrap_or_else(|_| "open_id".to_string());

            let result = super::chats::p2p_chat_id(client, &user_id, &id_type)
                .await.map_err(|e| ServiceError::internal(format!("{e}")))?;
            wrap(result)
        }
        "help" | "--help" | "-h" => Ok(json!({
            "output": HELP,
            "exit_code": 0,
        })),
        _ => Err(ServiceError::bad_request(format!("tab-im: 未知命令 '{subcmd}'"))),
    }
}

const HELP: &str = r#"tab-im — 消息操作

命令:
  list-messages    列出消息     --chat <id> [--id-type chat_id] [--limit 20]
  search-messages  搜索消息     --query <关键词> [--type text] [--chat-type group] [--limit 20]
  send-message     发送消息     --to <id> [--id-type chat_id] [--type text] --content <JSON>
  reply-message    回复消息     --message-id <id> [--type text] --content <JSON>
  list-chats       列出群组     [--limit 20] [--max-pages 3]
  search-chats     搜索群组     --query <关键词> [--limit 20]
  chat-info        群组详情     --chat <id>
  chat-members     群组成员     --chat <id> [--limit 100]
  p2p-chatid       获取单聊ID   --user <user_id> [--id-type open_id]

示例:
  tab-im list-chats
  tab-im search-messages --query "周报"
  tab-im send-message --to oc_xxxx --content '{"text":"你好"}'
  tab-im reply-message --message-id om_xxxx --content '{"text":"收到"}'
  tab-im p2p-chatid --user ou_xxxx
"#;

fn named_arg(args: &[String], flag: &str) -> Result<String, ServiceError> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::bad_request(format!("缺少参数 {flag}")))
}

fn parse_int(args: &[String], flag: &str, default: i32) -> i32 {
    named_arg(args, flag).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn wrap(data: Value) -> ServiceResult {
    let output = serde_json::to_string(&data).unwrap_or_default();
    Ok(json!({"output": output, "exit_code": 0}))
}
