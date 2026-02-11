use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("No events found")]
    NotFound,

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Kafka error: {0}")]
    KafkaError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Internal server error: {0}")]
    InternalError(String),
}

impl IntoResponse for HistoryError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            HistoryError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            
            HistoryError::DatabaseError(ref e) => {
                tracing::error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database operation failed".to_string(),
                )
            }
            
            HistoryError::KafkaError(ref e) => {
                tracing::error!("Kafka error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Event processing failed".to_string(),
                )
            }
            
            HistoryError::SerializationError(ref e) => {
                tracing::error!("Serialization error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to process event".to_string(),
                )
            }
            
            HistoryError::InternalError(ref e) => {
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

pub type HistoryResult<T> = Result<T, HistoryError>;
