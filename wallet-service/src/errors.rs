use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Application-level errors
/// 
/// Why thiserror?
/// - Derives Display, Error traits automatically
/// - Clean error messages
/// - Type-safe error handling
/// 
/// Why these specific errors?
/// - Each represents a distinct business failure
/// - HTTP status codes map naturally to these
#[derive(Debug, Error)]
pub enum WalletError {
    #[error("Wallet not found: {0}")]
    WalletNotFound(String),

    #[error("Insufficient balance. Required: {required}, Available: {available}")]
    InsufficientBalance {
        required: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    },

    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    #[error("Concurrent update detected. Please retry.")]
    OptimisticLockError,

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Kafka error: {0}")]
    KafkaError(String),

    #[error("Internal server error: {0}")]
    InternalError(String),
}

/// Convert WalletError to HTTP responses
/// 
/// This is where business errors become API responses
/// Key insight: Not all errors are 500s!
impl IntoResponse for WalletError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            WalletError::WalletNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            
            WalletError::InsufficientBalance { .. } => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            
            WalletError::InvalidAmount(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            
            WalletError::OptimisticLockError => {
                (StatusCode::CONFLICT, self.to_string())
            }
            
            WalletError::DatabaseError(ref e) => {
                tracing::error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database operation failed".to_string(),
                )
            }
            
            WalletError::KafkaError(ref e) => {
                // Log but don't expose Kafka details to client
                tracing::error!("Kafka error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Event publishing failed".to_string(),
                )
            }
            
            WalletError::InternalError(ref e) => {
                tracing::error!("Internal error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An unexpected error occurred".to_string(),
                )
            }
        };

        let body = Json(json!({
            "success": false,
            "error": error_message,
        }));

        (status, body).into_response()
    }
}

/// Helper type for Results in this application
pub type WalletResult<T> = Result<T, WalletError>;
