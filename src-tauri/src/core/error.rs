// 统一错误类型 — 对应 Python app/core/exceptions.py
//
// 所有 service 共享的错误体系

use serde::Serialize;

/// 服务层统一错误
#[derive(Debug, Serialize)]
pub struct ServiceError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// 错误码 — 对应 HTTP 状态码语义
#[derive(Debug, Clone, Copy, Serialize)]
pub enum ErrorCode {
    BadRequest,        // 400
    Unauthorized,      // 401
    Forbidden,         // 403
    NotFound,          // 404
    Timeout,           // 408
    Conflict,          // 409
    Internal,          // 500
    ServiceUnavailable, // 503
}

impl ServiceError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self { code: ErrorCode::BadRequest, message: msg.into(), data: None }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { code: ErrorCode::NotFound, message: msg.into(), data: None }
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self { code: ErrorCode::Unauthorized, message: msg.into(), data: None }
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self { code: ErrorCode::Forbidden, message: msg.into(), data: None }
    }
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self { code: ErrorCode::Timeout, message: msg.into(), data: None }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self { code: ErrorCode::Internal, message: msg.into(), data: None }
    }
    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self { code: ErrorCode::ServiceUnavailable, message: msg.into(), data: None }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}

/// 便捷 Result 别名
pub type ServiceResult<T = serde_json::Value> = Result<T, ServiceError>;
