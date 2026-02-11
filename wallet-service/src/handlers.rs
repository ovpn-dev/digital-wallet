use crate::errors::WalletResult;
use crate::kafka::KafkaProducer;
use crate::models::*;
use crate::repository::WalletRepository;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

/// Application state shared across handlers
/// 
/// Why Arc?
/// - Multiple async tasks need access
/// - Arc = Atomic Reference Counted smart pointer
/// - Thread-safe, cheap to clone
#[derive(Clone)]
pub struct AppState {
    pub repository: WalletRepository,
    pub kafka_producer: Arc<KafkaProducer>,
}

/// Create a new wallet
/// 
/// Flow:
/// 1. Create wallet in database
/// 2. Publish event to Kafka
/// 3. Return wallet to client
/// 
/// What if Kafka fails?
/// - Wallet exists in DB but no event published
/// - History service won't know about it
/// - This is the distributed systems problem we discussed!
pub async fn create_wallet(
    State(state): State<AppState>,
    Json(payload): Json<CreateWalletRequest>,
) -> WalletResult<Json<ApiResponse<WalletResponse>>> {
    tracing::info!(user_id = %payload.user_id, "Creating wallet");

    // Create wallet in database
    let wallet = state.repository.create_wallet(&payload.user_id).await?;

    // Publish event (if this fails, we return error but wallet already exists!)
    state
        .kafka_producer
        .publish_wallet_created(&wallet)
        .await?;

    tracing::info!(
        wallet_id = %wallet.id,
        user_id = %wallet.user_id,
        "Wallet created successfully"
    );

    Ok(Json(ApiResponse::success(WalletResponse::from(wallet))))
}

/// Get wallet by ID
pub async fn get_wallet(
    State(state): State<AppState>,
    Path(wallet_id): Path<String>,
) -> WalletResult<Json<ApiResponse<WalletResponse>>> {
    tracing::debug!(wallet_id = %wallet_id, "Fetching wallet");

    let wallet = state.repository.find_by_id(&wallet_id).await?;

    Ok(Json(ApiResponse::success(WalletResponse::from(wallet))))
}

/// Get all wallets for a user
pub async fn get_user_wallets(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> WalletResult<Json<ApiResponse<Vec<WalletResponse>>>> {
    tracing::debug!(user_id = %user_id, "Fetching user wallets");

    let wallets = state.repository.find_by_user_id(&user_id).await?;

    let response: Vec<WalletResponse> =
        wallets.into_iter().map(WalletResponse::from).collect();

    Ok(Json(ApiResponse::success(response)))
}

/// Fund a wallet (add money)
/// 
/// Flow:
/// 1. Update wallet balance in database (with optimistic locking)
/// 2. Create transaction record
/// 3. Publish event to Kafka
/// 4. Return updated wallet
/// 
/// Retry handling:
/// - If OptimisticLockError, client should retry
/// - Database guarantees consistency
/// - Event published only after DB commit succeeds
pub async fn fund_wallet(
    State(state): State<AppState>,
    Path(wallet_id): Path<String>,
    Json(payload): Json<FundWalletRequest>,
) -> WalletResult<Json<ApiResponse<WalletResponse>>> {
    tracing::info!(
        wallet_id = %wallet_id,
        amount = %payload.amount,
        "Funding wallet"
    );

    // Update database (atomic operation)
    let (wallet, transaction) = state
        .repository
        .fund_wallet(&wallet_id, payload.amount)
        .await?;

    // Publish event
    state
        .kafka_producer
        .publish_wallet_funded(&wallet, payload.amount, transaction.id)
        .await?;

    tracing::info!(
        wallet_id = %wallet_id,
        new_balance = %wallet.balance,
        "Wallet funded successfully"
    );

    Ok(Json(ApiResponse::success(WalletResponse::from(wallet))))
}

/// Transfer money between wallets
/// 
/// Flow:
/// 1. Lock both wallets in database
/// 2. Validate sender has sufficient balance
/// 3. Update both balances
/// 4. Create two transaction records (outgoing + incoming)
/// 5. Publish event to Kafka
/// 6. Return both transaction records
/// 
/// Critical points:
/// - Everything happens in a single DB transaction
/// - Wallets locked in consistent order (prevents deadlock)
/// - Event published only after successful commit
pub async fn transfer(
    State(state): State<AppState>,
    Path(from_wallet_id): Path<String>,
    Json(payload): Json<TransferRequest>,
) -> WalletResult<Json<ApiResponse<Vec<TransactionResponse>>>> {
    tracing::info!(
        from_wallet_id = %from_wallet_id,
        to_wallet_id = %payload.to_wallet_id,
        amount = %payload.amount,
        "Processing transfer"
    );

    // Get the "from" wallet details for the event
    let from_wallet = state.repository.find_by_id(&from_wallet_id).await?;
    let to_wallet = state.repository.find_by_id(&payload.to_wallet_id).await?;

    // Execute transfer (atomic operation)
    let (out_txn, in_txn) = state
        .repository
        .transfer(&from_wallet_id, &payload.to_wallet_id, payload.amount)
        .await?;

    // Publish event
    state
        .kafka_producer
        .publish_transfer_completed(
            from_wallet.id.clone(),
            from_wallet.user_id.clone(),
            to_wallet.id.clone(),
            to_wallet.user_id.clone(),
            payload.amount,
            out_txn.reference_id.clone().unwrap_or_default(),
        )
        .await?;

    tracing::info!(
        from_wallet_id = %from_wallet_id,
        to_wallet_id = %payload.to_wallet_id,
        amount = %payload.amount,
        "Transfer completed successfully"
    );

    let response = vec![
        TransactionResponse::from(out_txn),
        TransactionResponse::from(in_txn),
    ];

    Ok(Json(ApiResponse::success(response)))
}

/// Health check endpoint
/// 
/// Returns 200 if service is healthy
/// Can be extended to check database and Kafka connectivity
pub async fn health_check() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}
