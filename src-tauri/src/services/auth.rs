// Auth 服务 — JWT/Ticket 认证
//
// 对应 Python app/services/auth.py
// 纯 Rust 实现, 不依赖 pilot/auth

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::core::error::{ServiceError, ServiceResult};

/// 认证配置
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// JWT 公钥 (PEM)
    pub jwt_public_key: Option<String>,
    /// Ticket 密钥
    pub ticket_secret: Option<String>,
    /// Ticket 有效期 (秒)
    pub ticket_ttl: u64,
    /// 是否启用认证
    pub enabled: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_public_key: std::env::var("JWT_PUBLIC_KEY").ok(),
            ticket_secret: std::env::var("TICKET_SECRET").ok(),
            ticket_ttl: std::env::var("TICKET_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            enabled: std::env::var("AUTH_ENABLED")
                .map(|v| v != "0" && v.to_lowercase() != "false")
                .unwrap_or(false),
        }
    }
}

/// Ticket 记录
struct TicketRecord {
    account_id: String,
    created_at: Instant,
}

/// Auth 服务
pub struct AuthService {
    config: AuthConfig,
    tickets: RwLock<HashMap<String, TicketRecord>>,
}

impl AuthService {
    pub fn new() -> Self {
        let config = AuthConfig::default();
        log::info!(
            "[Auth] 初始化 (enabled={}, has_jwt_key={}, has_ticket_secret={})",
            config.enabled,
            config.jwt_public_key.is_some(),
            config.ticket_secret.is_some()
        );
        Self {
            config,
            tickets: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_config(config: AuthConfig) -> Self {
        Self {
            config,
            tickets: RwLock::new(HashMap::new()),
        }
    }

    /// 创建 Ticket
    pub async fn create_ticket(&self, account_id: &str) -> ServiceResult {
        if !self.config.enabled {
            return Err(ServiceError::unavailable("认证未启用"));
        }

        let ticket = uuid::Uuid::new_v4().to_string();
        let record = TicketRecord {
            account_id: account_id.to_string(),
            created_at: Instant::now(),
        };

        self.tickets.write().await.insert(ticket.clone(), record);

        Ok(json!({
            "ticket": ticket,
            "ttl": self.config.ticket_ttl,
            "account_id": account_id,
        }))
    }

    /// 验证 Ticket
    pub async fn validate_ticket(&self, ticket: &str) -> ServiceResult {
        let tickets = self.tickets.read().await;
        let record = tickets
            .get(ticket)
            .ok_or_else(|| ServiceError::unauthorized("无效 ticket"))?;

        if record.created_at.elapsed() > Duration::from_secs(self.config.ticket_ttl) {
            drop(tickets);
            self.tickets.write().await.remove(ticket);
            return Err(ServiceError::unauthorized("ticket 已过期"));
        }

        Ok(json!({
            "valid": true,
            "account_id": record.account_id,
        }))
    }

    /// 验证 JWT (简化版 — 仅解码 payload, 不验证签名)
    pub fn validate_jwt(&self, token: &str) -> ServiceResult {
        if !self.config.enabled {
            return Ok(json!({"valid": true, "reason": "auth_disabled"}));
        }

        // JWT 格式: header.payload.signature
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(ServiceError::unauthorized("无效 JWT 格式"));
        }

        // 解码 payload (base64url)
        let payload_bytes = base64_url_decode(parts[1])
            .map_err(|_| ServiceError::unauthorized("JWT payload 解码失败"))?;

        let payload: Value = serde_json::from_slice(&payload_bytes)
            .map_err(|_| ServiceError::unauthorized("JWT payload 解析失败"))?;

        // 检查过期
        if let Some(exp) = payload["exp"].as_u64() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now > exp {
                return Err(ServiceError::unauthorized("JWT 已过期"));
            }
        }

        Ok(json!({
            "valid": true,
            "payload": payload,
        }))
    }

    /// 清理过期 ticket
    pub async fn cleanup_expired(&self) -> usize {
        let mut tickets = self.tickets.write().await;
        let ttl = Duration::from_secs(self.config.ticket_ttl);
        let before = tickets.len();
        tickets.retain(|_, r| r.created_at.elapsed() <= ttl);
        before - tickets.len()
    }

    /// WS handler
    pub async fn handle(&self, action: &str, params: Value) -> ServiceResult {
        match action {
            "create_ticket" => {
                let account_id = params["account_id"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 account_id"))?;
                self.create_ticket(account_id).await
            }
            "validate_ticket" => {
                let ticket = params["ticket"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 ticket"))?;
                self.validate_ticket(ticket).await
            }
            "validate_jwt" => {
                let token = params["token"]
                    .as_str()
                    .ok_or_else(|| ServiceError::bad_request("缺少 token"))?;
                self.validate_jwt(token)
            }
            "cleanup" => {
                let count = self.cleanup_expired().await;
                Ok(json!({"cleaned": count}))
            }
            _ => Err(ServiceError::bad_request(format!("未知 auth 操作: {action}"))),
        }
    }
}

/// Base64url 解码
fn base64_url_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let padded = match input.len() % 4 {
        2 => format!("{input}=="),
        3 => format!("{input}="),
        _ => input.to_string(),
    };
    let standard = padded.replace('-', "+").replace('_', "/");
    base64::engine::general_purpose::STANDARD
        .decode(standard)
        .map_err(|e| e.to_string())
}
