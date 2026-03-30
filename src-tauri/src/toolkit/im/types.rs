// toolkit/im/types — 消息类型定义

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 消息内容类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum MessageContent {
    Text(String),
    RichText(Value),       // Lark docx 富文本
    Image(ImageContent),
    File(FileContent),
    Audio(AudioContent),
    Video(VideoContent),
    Sticker(String),       // sticker_id
    ShareChat(String),     // chat_id
    ShareUser(String),     // user_id
    Post(Value),           // 帖子
    Interactive(Value),    // 卡片消息
    MergeForward(Vec<Value>),
    Location(LocationContent),
    Unknown(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageContent {
    pub image_key: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub file_key: String,
    pub file_name: String,
    #[serde(default)]
    pub file_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioContent {
    pub file_key: String,
    #[serde(default)]
    pub duration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoContent {
    pub file_key: String,
    #[serde(default)]
    pub image_key: String,
    #[serde(default)]
    pub duration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationContent {
    pub name: String,
    pub longitude: f64,
    pub latitude: f64,
}

/// 解析消息内容
pub fn parse_content(msg_type: &str, content: &Value) -> MessageContent {
    match msg_type {
        "text" => {
            let text = content
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            MessageContent::Text(text.to_string())
        }
        "post" | "rich_text" => MessageContent::RichText(content.clone()),
        "image" => {
            let key = content
                .get("image_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            MessageContent::Image(ImageContent {
                image_key: key,
                width: content.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                height: content.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            })
        }
        "file" => MessageContent::File(FileContent {
            file_key: content.get("file_key").and_then(|v| v.as_str()).unwrap_or("").into(),
            file_name: content.get("file_name").and_then(|v| v.as_str()).unwrap_or("").into(),
            file_size: content.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0),
        }),
        "audio" => MessageContent::Audio(AudioContent {
            file_key: content.get("file_key").and_then(|v| v.as_str()).unwrap_or("").into(),
            duration: content.get("duration").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        }),
        "media" | "video" => MessageContent::Video(VideoContent {
            file_key: content.get("file_key").and_then(|v| v.as_str()).unwrap_or("").into(),
            image_key: content.get("image_key").and_then(|v| v.as_str()).unwrap_or("").into(),
            duration: content.get("duration").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        }),
        "sticker" => {
            let id = content.get("file_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
            MessageContent::Sticker(id)
        }
        "share_chat" => {
            let id = content.get("chat_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            MessageContent::ShareChat(id)
        }
        "share_user" => {
            let id = content.get("user_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            MessageContent::ShareUser(id)
        }
        "interactive" => MessageContent::Interactive(content.clone()),
        "merge_forward" => {
            let messages = content
                .get("messages")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            MessageContent::MergeForward(messages)
        }
        "location" => MessageContent::Location(LocationContent {
            name: content.get("name").and_then(|v| v.as_str()).unwrap_or("").into(),
            longitude: content.get("longitude").and_then(|v| v.as_f64()).unwrap_or(0.0),
            latitude: content.get("latitude").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }),
        _ => MessageContent::Unknown(content.clone()),
    }
}
