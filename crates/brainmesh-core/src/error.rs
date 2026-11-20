use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum BrainMeshError {
    #[error("authentication failed: {0}")]
    Authentication(String),

    #[error("permission denied: {0}")]
    Authorization(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("rate limit exceeded: {0}")]
    RateLimited(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl BrainMeshError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Authentication(_) => "AUTHENTICATION_ERROR",
            Self::Authorization(_) => "AUTHORIZATION_ERROR",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::RateLimited(_) => "RATE_LIMITED",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn http_status_code(&self) -> u16 {
        match self {
            Self::Authentication(_) => 401,
            Self::Authorization(_) => 403,
            Self::Validation(_) => 400,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::RateLimited(_) => 429,
            Self::Storage(_) | Self::Internal(_) => 500,
        }
    }
}

pub type Result<T> = std::result::Result<T, BrainMeshError>;
