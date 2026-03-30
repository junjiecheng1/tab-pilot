// toolkit/im/format — 消息格式化输出
//
// 移植自: aily_im/commands/_format.py (301行)

use serde_json::Value;

use super::types::{self, MessageContent};

/// 格式化单条消息为可读字符串
pub fn format_message(msg: &Value) -> String {
    let sender = msg
        .get("sender")
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let create_time = msg
        .get("create_time")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let msg_type = msg
        .get("msg_type")
        .and_then(|v| v.as_str())
        .unwrap_or("text");

    // 解析 body.content JSON
    let content_str = msg
        .get("body")
        .and_then(|b| b.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("{}");

    let content_val: Value = serde_json::from_str(content_str).unwrap_or(Value::Object(
        serde_json::Map::new(),
    ));

    let parsed = types::parse_content(msg_type, &content_val);
    let content_text = format_content(&parsed);

    format!("[{create_time}] {sender}: {content_text}")
}

/// 格式化消息内容
fn format_content(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::RichText(val) => extract_rich_text(val),
        MessageContent::Image(img) => format!("[图片 {}]", img.image_key),
        MessageContent::File(file) => format!("[文件 {} ({} bytes)]", file.file_name, file.file_size),
        MessageContent::Audio(audio) => format!("[语音 {}s]", audio.duration),
        MessageContent::Video(video) => format!("[视频 {}s]", video.duration),
        MessageContent::Sticker(id) => format!("[表情 {id}]"),
        MessageContent::ShareChat(id) => format!("[分享群聊 {id}]"),
        MessageContent::ShareUser(id) => format!("[分享用户 {id}]"),
        MessageContent::Post(val) => extract_post_text(val),
        MessageContent::Interactive(val) => {
            val.get("header")
                .and_then(|h| h.get("title"))
                .and_then(|t| t.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("[卡片消息]")
                .to_string()
        }
        MessageContent::MergeForward(msgs) => format!("[合并转发 {} 条]", msgs.len()),
        MessageContent::Location(loc) => format!("[位置 {}]", loc.name),
        MessageContent::Unknown(_) => "[未知消息]".to_string(),
    }
}

/// 从富文本中提取纯文本
fn extract_rich_text(val: &Value) -> String {
    let mut parts = Vec::new();

    // 遍历所有语言版本
    if let Some(obj) = val.as_object() {
        for (_, content) in obj {
            if let Some(paragraphs) = content.as_array() {
                for para in paragraphs {
                    if let Some(elements) = para.as_array() {
                        for elem in elements {
                            if let Some(text) = elem.get("text").and_then(|v| v.as_str()) {
                                parts.push(text.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    parts.join("")
}

/// 从帖子中提取文本
fn extract_post_text(val: &Value) -> String {
    let title = val
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut body_parts = Vec::new();
    if let Some(content) = val.get("content").and_then(|v| v.as_array()) {
        for para in content {
            if let Some(elements) = para.as_array() {
                for elem in elements {
                    if let Some(text) = elem.get("text").and_then(|v| v.as_str()) {
                        body_parts.push(text.to_string());
                    }
                }
            }
        }
    }

    if title.is_empty() {
        body_parts.join("")
    } else {
        format!("{title}: {}", body_parts.join(""))
    }
}

/// 批量格式化消息
pub fn format_messages(messages: &[Value]) -> Vec<String> {
    messages.iter().map(format_message).collect()
}

/// 格式化附件
/// 移植自: aily_im/commands/_format.py _expand_attachments
fn format_attachment(att: &Value) -> String {
    let att_type = att
        .get("type")
        .or_else(|| att.get("msg_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("file");
    let name = att
        .get("name")
        .or_else(|| att.get("file_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    
    match att_type {
        "image" => format!("  📷 [图片]{}", if name.is_empty() { String::new() } else { format!(" {name}") }),
        "file" => {
            let size = att.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            let size_str = if size > 0 { format!(" ({}KB)", size / 1024) } else { String::new() };
            format!("  📎 [文件] {name}{size_str}")
        }
        "audio" => "  🎵 [语音]".to_string(),
        "media" | "video" => "  📹 [视频]".to_string(),
        "sticker" => "  😀 [表情]".to_string(),
        _ => format!("  📄 [{att_type}] {name}"),
    }
}

/// 短时间格式 (Unix ms → "MM-dd HH:mm")
/// 移植自: aily_im/commands/_format.py _short_time
fn short_time(val: &Value) -> String {
    let ts = match val {
        Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        _ => return String::new(),
    };
    if ts == 0.0 {
        return String::new();
    }
    let secs = if ts > 1e12 { (ts / 1000.0) as i64 } else { ts as i64 };
    // 简化格式 — 使用 chrono 如果可用，否则用原始时间戳
    format!("{secs}")
}

/// 线程化格式 — 消息按回复关系分组
/// 移植自: aily_im/commands/_format.py format_all + _format_thread
pub fn format_threaded(messages: &[Value]) -> String {
    use std::collections::{HashMap, HashSet};
    
    let mut id_to_msg: HashMap<String, &Value> = HashMap::new();
    let mut threads: HashMap<String, Vec<&Value>> = HashMap::new();
    
    // 建索引
    for msg in messages {
        let msg_id = msg.get("message_id").and_then(|v| v.as_str()).unwrap_or("");
        if !msg_id.is_empty() {
            id_to_msg.insert(msg_id.to_string(), msg);
        }
        
        let parent = msg
            .get("parent_id")
            .or_else(|| msg.get("root_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !parent.is_empty() {
            threads.entry(parent.to_string()).or_default().push(msg);
        }
    }
    
    let mut seen: HashSet<String> = HashSet::new();
    let mut lines: Vec<String> = Vec::new();
    
    // 先输出话题
    for (parent_id, replies) in &threads {
        if let Some(root) = id_to_msg.get(parent_id) {
            seen.insert(parent_id.clone());
            for r in replies {
                if let Some(rid) = r.get("message_id").and_then(|v| v.as_str()) {
                    seen.insert(rid.to_string());
                }
            }
            
            lines.push(format!("💬 {}", format_message(root)));
            for r in replies {
                lines.push(format!("  ↳ {}", format_message(r)));
            }
            lines.push(String::new());
        }
    }
    
    // 独立消息
    for msg in messages {
        let msg_id = msg.get("message_id").and_then(|v| v.as_str()).unwrap_or("");
        if !seen.contains(msg_id) {
            lines.push(format_message(msg));
        }
    }
    
    lines.join("\n")
}

/// 批量格式化多个群聊
/// 移植自: aily_im/commands/_format.py format_listed_messages
pub fn format_listed_chats(chats: &[Value]) -> String {
    let mut parts: Vec<String> = Vec::new();
    
    for chat in chats {
        let name = chat.get("name").and_then(|v| v.as_str()).unwrap_or("未命名群聊");
        let messages = chat.get("messages").and_then(|v| v.as_array());
        
        let body = match messages {
            Some(msgs) if !msgs.is_empty() => format_threaded(msgs),
            _ => "（无消息）".to_string(),
        };
        
        parts.push(format!("## {name}\n{body}"));
    }
    
    parts.join("\n\n---\n\n")
}
