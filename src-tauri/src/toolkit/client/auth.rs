// toolkit/client/auth — 从后端获取飞书 token (TAT + UAT)
//
// 流程: TabPilot → GET backend/api/feishu/token → { tat, uat, ... }
// Connector 的 app_id/secret → TAT
// OAuthCredential → UAT（过期自动 refresh）

use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 飞书 Token 响应
#[derive(Debug, Deserialize)]
pub struct FeishuTokens {
    /// tenant_access_token (应用级)
    pub tat: String,
    /// user_access_token (用户级, 可为 null)
    pub uat: Option<String>,
    pub has_uat: bool,
    pub uat_expires_in: u64,
    pub connector_id: String,
    pub platform: String,
}

/// Token 提供者 — 从后端服务器获取飞书 token
#[derive(Clone)]
pub struct TokenProvider {
    backend_url: String,
    /// 认证用的 JWT token (登录后端用)
    auth_token: Arc<RwLock<Option<String>>>,
    cache: Arc<RwLock<TokenCache>>,
    http: reqwest::Client,
}

struct TokenCache {
    tokens: Option<FeishuTokens>,
    fetched_at: Option<Instant>,
}

impl TokenProvider {
    /// 从环境变量创建
    pub fn from_env() -> Self {
        let backend_url =
            std::env::var("BACKEND_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
        Self::new(backend_url)
    }

    /// 指定后端地址
    pub fn new(backend_url: impl Into<String>) -> Self {
        Self {
            backend_url: backend_url.into(),
            auth_token: Arc::new(RwLock::new(None)),
            cache: Arc::new(RwLock::new(TokenCache {
                tokens: None,
                fetched_at: None,
            })),
            http: reqwest::Client::new(),
        }
    }

    /// 设置后端认证 token (JWT)
    pub async fn set_auth_token(&self, token: impl Into<String>) {
        let mut guard = self.auth_token.write().await;
        *guard = Some(token.into());
    }

    /// 获取 TAT (tenant_access_token)
    pub async fn get_tat(&self) -> Result<String, super::TabClientError> {
        let tokens = self.get_tokens().await?;
        Ok(tokens.tat)
    }

    /// 获取 UAT (user_access_token), 可能为 None
    pub async fn get_uat(&self) -> Result<Option<String>, super::TabClientError> {
        let tokens = self.get_tokens().await?;
        Ok(tokens.uat)
    }

    /// 创建使用 TAT 的 TabClient
    pub async fn create_client(&self) -> Result<super::TabClient, super::TabClientError> {
        let tat = self.get_tat().await?;
        Ok(super::TabClient::new(tat))
    }

    /// 创建使用 UAT 的 TabClient (代表用户操作)
    pub async fn create_user_client(&self) -> Result<super::TabClient, super::TabClientError> {
        let uat = self
            .get_uat()
            .await?
            .ok_or_else(|| super::TabClientError::Other("用户未授权飞书，UAT 不可用".into()))?;
        Ok(super::TabClient::new(uat))
    }

    /// 获取完整 token 信息
    async fn get_tokens(&self) -> Result<FeishuTokens, super::TabClientError> {
        // 缓存有效期: UAT 过期前 5 分钟, 或 30 分钟 (TAT)
        {
            let cache = self.cache.read().await;
            if let (Some(ref tokens), Some(fetched_at)) = (&cache.tokens, cache.fetched_at) {
                let age = fetched_at.elapsed();
                let max_age = if tokens.has_uat {
                    Duration::from_secs(tokens.uat_expires_in.saturating_sub(300))
                } else {
                    Duration::from_secs(1800) // TAT only: 30 min
                };
                if age < max_age {
                    return Ok(FeishuTokens {
                        tat: tokens.tat.clone(),
                        uat: tokens.uat.clone(),
                        has_uat: tokens.has_uat,
                        uat_expires_in: tokens.uat_expires_in.saturating_sub(age.as_secs()),
                        connector_id: tokens.connector_id.clone(),
                        platform: tokens.platform.clone(),
                    });
                }
            }
        }

        self.refresh_tokens().await
    }

    async fn refresh_tokens(&self) -> Result<FeishuTokens, super::TabClientError> {
        let url = format!("{}/api/feishu/token", self.backend_url);

        let mut req = self.http.get(&url).timeout(Duration::from_secs(10));

        // 带上后端认证
        {
            let guard = self.auth_token.read().await;
            if let Some(ref token) = *guard {
                req = req.bearer_auth(token);
            }
        }

        let resp = req.send().await.map_err(super::TabClientError::Http)?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(super::TabClientError::Other(format!(
                "后端返回 {status}: {body}"
            )));
        }

        let tokens: FeishuTokens = resp.json().await.map_err(super::TabClientError::Http)?;

        log::info!(
            "[TokenProvider] 获取飞书 token 成功 (TAT=✓, UAT={}, connector={})",
            if tokens.has_uat { "✓" } else { "✗" },
            tokens.connector_id
        );

        // 缓存
        {
            let mut cache = self.cache.write().await;
            cache.tokens = Some(FeishuTokens {
                tat: tokens.tat.clone(),
                uat: tokens.uat.clone(),
                has_uat: tokens.has_uat,
                uat_expires_in: tokens.uat_expires_in,
                connector_id: tokens.connector_id.clone(),
                platform: tokens.platform.clone(),
            });
            cache.fetched_at = Some(Instant::now());
        }

        Ok(tokens)
    }
}
