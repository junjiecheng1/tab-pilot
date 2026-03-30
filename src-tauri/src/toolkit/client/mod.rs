// toolkit/client — 飞书 OpenAPI 客户端
//
// 移植自: aily_client/client.py (193行)
// 改动: AilyClient → TabClient, aily内部API → 飞书 OpenAPI

pub mod auth;
pub mod bitable;
pub mod im;
pub mod calendar;
pub mod doc;
pub mod drive;
pub mod user;

use std::time::Duration;

use reqwest::{Client, Response};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

/// 飞书 API 统一响应格式
#[derive(Debug, Deserialize)]
pub struct FeishuResponse<T> {
    pub code: i64,
    #[serde(default)]
    pub msg: String,
    #[serde(default)]
    pub data: Option<T>,
}

impl<T> FeishuResponse<T> {
    pub fn into_result(self) -> std::result::Result<T, TabClientError> {
        if self.code != 0 {
            return Err(TabClientError::Api {
                code: self.code,
                msg: self.msg,
            });
        }
        self.data.ok_or(TabClientError::EmptyData)
    }
}

/// 分页数据
#[derive(Debug, Default, Deserialize)]
pub struct PageData<T> {
    #[serde(default)]
    pub items: Vec<T>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub page_token: Option<String>,
    #[serde(default)]
    pub total: Option<i64>,
}

/// 客户端错误
#[derive(Debug, thiserror::Error)]
pub enum TabClientError {
    #[error("飞书 API 错误 (code={code}): {msg}")]
    Api { code: i64, msg: String },

    #[error("HTTP 请求错误: {0}")]
    Http(#[from] reqwest::Error),

    #[error("响应数据为空")]
    EmptyData,

    #[error("参数错误: {0}")]
    InvalidParam(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, TabClientError>;

/// Tab 飞书 OpenAPI 客户端
#[derive(Clone)]
pub struct TabClient {
    http: Client,
    base_url: String,
    token: String,
    timeout: Duration,
}

impl TabClient {
    /// 创建客户端
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: "https://open.feishu.cn/open-apis".to_string(),
            token: token.into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// 自定义 base_url
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// 自定义超时
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 自定义 HTTP 客户端
    pub fn with_http_client(mut self, client: Client) -> Self {
        self.http = client;
        self
    }

    /// 更新 token
    pub fn set_token(&mut self, token: impl Into<String>) {
        self.token = token.into();
    }

    // ── 底层请求 ──────────────────────────────

    /// GET 请求
    pub async fn get<T: DeserializeOwned + Default>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .query(params)
            .timeout(self.timeout)
            .send()
            .await?;
        self.parse_response(resp).await
    }

    /// POST 请求 (JSON body)
    pub async fn post<T: DeserializeOwned + Default>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(body)
            .timeout(self.timeout)
            .send()
            .await?;
        self.parse_response(resp).await
    }

    /// PUT 请求
    pub async fn put<T: DeserializeOwned + Default>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .put(&url)
            .bearer_auth(&self.token)
            .json(body)
            .timeout(self.timeout)
            .send()
            .await?;
        self.parse_response(resp).await
    }

    /// DELETE 请求
    pub async fn delete<T: DeserializeOwned + Default>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .delete(&url)
            .bearer_auth(&self.token)
            .json(body)
            .timeout(self.timeout)
            .send()
            .await?;
        self.parse_response(resp).await
    }

    /// POST multipart (文件上传)
    pub async fn post_multipart<T: DeserializeOwned + Default>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .multipart(form)
            .timeout(self.timeout)
            .send()
            .await?;
        self.parse_response(resp).await
    }

    /// GET 下载 (返回 bytes)
    pub async fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .timeout(self.timeout)
            .send()
            .await?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// GET 请求（返回原始 JSON Value）
    pub async fn get_raw(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<Value> {
        self.get::<Value>(path, params).await
    }

    /// POST 请求（返回原始 JSON Value）
    pub async fn post_raw(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<Value> {
        self.post::<Value>(path, body).await
    }

    // ── 分页辅助 ──────────────────────────────

    /// 自动分页获取所有数据
    pub async fn get_all_pages<T: DeserializeOwned + Default>(
        &self,
        path: &str,
        base_params: &[(&str, String)],
    ) -> Result<Vec<T>> {
        let mut all_items: Vec<T> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut params: Vec<(&str, String)> = base_params.to_vec();
            if let Some(ref token) = page_token {
                params.push(("page_token", token.clone()));
            }

            // 转换为 &str 对
            let str_params: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();

            let page: PageData<T> = self.get(path, &str_params).await?;
            all_items.extend(page.items);

            if !page.has_more {
                break;
            }
            page_token = page.page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(all_items)
    }

    // ── 内部 ──────────────────────────────

    async fn parse_response<T: DeserializeOwned + Default>(
        &self,
        resp: Response,
    ) -> Result<T> {
        let feishu: FeishuResponse<T> = resp.json().await?;
        feishu.into_result()
    }
}

/// 递归压缩值（调试用）
pub fn compact_value(val: &Value, depth: usize, max_items: usize) -> Value {
    match val {
        Value::Object(map) => {
            if map.len() > max_items {
                let mut out = serde_json::Map::new();
                for (k, v) in map.iter().take(max_items) {
                    out.insert(k.clone(), compact_value(v, depth + 1, max_items));
                }
                out.insert(
                    format!("...+{}", map.len() - max_items),
                    Value::String("...".into()),
                );
                Value::Object(out)
            } else {
                let out: serde_json::Map<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), compact_value(v, depth + 1, max_items)))
                    .collect();
                Value::Object(out)
            }
        }
        Value::Array(arr) => {
            if arr.len() > max_items {
                let mut out: Vec<Value> = arr
                    .iter()
                    .take(max_items)
                    .map(|v| compact_value(v, depth + 1, max_items))
                    .collect();
                out.push(Value::String(format!("...+{}", arr.len() - max_items)));
                Value::Array(out)
            } else {
                Value::Array(
                    arr.iter()
                        .map(|v| compact_value(v, depth + 1, max_items))
                        .collect(),
                )
            }
        }
        other => other.clone(),
    }
}
