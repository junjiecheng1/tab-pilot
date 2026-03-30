// toolkit/openai — OpenAI 兼容传输层
//
// 移植自: aily_openai_transport.py (96行)
// 提供 OpenAI 兼容的 chat/completions API
// 支持:
//   - 非流式 chat completion
//   - 流式 SSE completion (streaming)
//   - 端点到工具的映射 (endpoint_to_tool)

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// OpenAI 兼容的聊天请求
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub tools: Option<Vec<Value>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

/// OpenAI 兼容的聊天响应
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// 流式 SSE 响应块
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatChunk {
    pub id: String,
    pub object: String,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: usize,
    pub delta: ChunkDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<Value>>,
}

/// 端点名到工具名映射 (移植自 Python AilyTransport)
pub struct EndpointMapping {
    pub map: HashMap<String, String>,
}

impl EndpointMapping {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn with(mut self, endpoint: &str, tool: &str) -> Self {
        self.map.insert(endpoint.to_string(), tool.to_string());
        self
    }

    /// 根据路径查找工具名 (精确 → 去尾斜杠 → 加首斜杠)
    pub fn find_tool(&self, path: &str) -> String {
        if let Some(t) = self.map.get(path) {
            return t.clone();
        }
        let stripped = path.trim_end_matches('/');
        if let Some(t) = self.map.get(stripped) {
            return t.clone();
        }
        let with_prefix = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        if let Some(t) = self.map.get(&with_prefix) {
            return t.clone();
        }
        path.to_string()
    }
}

/// 发送非流式 ChatCompletion
pub async fn chat_completion(
    base_url: &str,
    api_key: &str,
    request: &ChatRequest,
) -> Result<ChatResponse, crate::toolkit::client::TabClientError> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(request)
        .send()
        .await
        .map_err(crate::toolkit::client::TabClientError::Http)?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::toolkit::client::TabClientError::Other(
            format!("API error {status}: {body}")
        ));
    }

    let response: ChatResponse = resp
        .json()
        .await
        .map_err(crate::toolkit::client::TabClientError::Http)?;

    Ok(response)
}

/// 发送流式 ChatCompletion，返回字节流
pub async fn chat_completion_stream(
    base_url: &str,
    api_key: &str,
    request: &ChatRequest,
) -> Result<reqwest::Response, crate::toolkit::client::TabClientError> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));

    let mut stream_request = serde_json::to_value(request)
        .map_err(|e| crate::toolkit::client::TabClientError::Other(e.to_string()))?;
    stream_request["stream"] = json!(true);

    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&stream_request)
        .send()
        .await
        .map_err(crate::toolkit::client::TabClientError::Http)?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::toolkit::client::TabClientError::Other(
            format!("API error {status}: {body}")
        ));
    }

    Ok(resp)
}
