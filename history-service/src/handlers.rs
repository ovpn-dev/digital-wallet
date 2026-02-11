use crate::errors::HistoryResult;
use crate::models::{ApiResponse, EventResponse};
use crate::repository::EventRepository;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

#[derive(Clone)]
pub struct AppState {
    pub repository: EventRepository,
}

/// Get transaction history for a specific wallet
/// 
/// Returns all events affecting this wallet in reverse chronological order
/// 
/// Example response:
/// [
///   {
///     "event_type": "TRANSFER_IN",
///     "amount": "30.0000",
///     "created_at": "2025-01-29T10:30:00Z"
///   },
///   {
///     "event_type": "WALLET_FUNDED",
///     "amount": "100.0000",
///     "created_at": "2025-01-29T10:00:00Z"
///   }
/// ]
pub async fn get_wallet_history(
    State(state): State<AppState>,
    Path(wallet_id): Path<String>,
) -> HistoryResult<Json<ApiResponse<Vec<EventResponse>>>> {
    tracing::debug!(wallet_id = %wallet_id, "Fetching wallet history");

    let events = state.repository.get_wallet_history(&wallet_id).await?;

    if events.is_empty() {
        tracing::info!(wallet_id = %wallet_id, "No events found for wallet");
    }

    let response: Vec<EventResponse> = events
        .into_iter()
        .map(EventResponse::from)
        .collect();

    Ok(Json(ApiResponse::success(response)))
}

/// Get all activity for a specific user
/// 
/// Returns events across ALL wallets owned by this user
/// Useful for showing "My Activity" page in a mobile app
pub async fn get_user_activity(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> HistoryResult<Json<ApiResponse<Vec<EventResponse>>>> {
    tracing::debug!(user_id = %user_id, "Fetching user activity");

    let events = state.repository.get_user_activity(&user_id).await?;

    if events.is_empty() {
        tracing::info!(user_id = %user_id, "No activity found for user");
    }

    let response: Vec<EventResponse> = events
        .into_iter()
        .map(EventResponse::from)
        .collect();

    Ok(Json(ApiResponse::success(response)))
}

/// Health check endpoint
pub async fn health_check() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}
