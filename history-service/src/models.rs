use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Transaction event stored in the database
/// This is our event-sourced history
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TransactionEvent {
    pub id: String,
    pub wallet_id: String,
    pub user_id: String,
    pub amount: Decimal,
    pub event_type: String,
    pub transaction_id: Option<String>, // For idempotency - ensures we don't process same event twice
    pub created_at: DateTime<Utc>,
    pub event_data: serde_json::Value, // JSONB - stores the full event for debugging
}

/// Wallet events from Kafka (matches what Wallet Service publishes)
/// 
/// These come from the wallet-events Kafka topic
/// We'll deserialize them and store in transaction_events table
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "eventType")]
pub enum WalletEvent {
    #[serde(rename = "WALLET_CREATED")]
    WalletCreated {
        wallet_id: String,
        user_id: String,
        timestamp: DateTime<Utc>,
    },

    #[serde(rename = "WALLET_FUNDED")]
    WalletFunded {
        wallet_id: String,
        user_id: String,
        amount: Decimal,
        new_balance: Decimal,
        transaction_id: String,
        timestamp: DateTime<Utc>,
    },

    #[serde(rename = "TRANSFER_COMPLETED")]
    TransferCompleted {
        from_wallet_id: String,
        from_user_id: String,
        to_wallet_id: String,
        to_user_id: String,
        amount: Decimal,
        reference_id: String,
        timestamp: DateTime<Utc>,
    },
}

impl WalletEvent {
    /// Get the event type as a string
    pub fn event_type(&self) -> &str {
        match self {
            WalletEvent::WalletCreated { .. } => "WALLET_CREATED",
            WalletEvent::WalletFunded { .. } => "WALLET_FUNDED",
            WalletEvent::TransferCompleted { .. } => "TRANSFER_COMPLETED",
        }
    }

    /// Get the primary wallet ID
    pub fn wallet_id(&self) -> &str {
        match self {
            WalletEvent::WalletCreated { wallet_id, .. } => wallet_id,
            WalletEvent::WalletFunded { wallet_id, .. } => wallet_id,
            WalletEvent::TransferCompleted { from_wallet_id, .. } => from_wallet_id,
        }
    }

    /// Get the user ID
    pub fn user_id(&self) -> &str {
        match self {
            WalletEvent::WalletCreated { user_id, .. } => user_id,
            WalletEvent::WalletFunded { user_id, .. } => user_id,
            WalletEvent::TransferCompleted { from_user_id, .. } => from_user_id,
        }
    }

    /// Get the transaction ID for idempotency (if available)
    pub fn transaction_id(&self) -> Option<String> {
        match self {
            WalletEvent::WalletCreated { .. } => None,
            WalletEvent::WalletFunded { transaction_id, .. } => Some(transaction_id.clone()),
            WalletEvent::TransferCompleted { reference_id, .. } => Some(reference_id.clone()),
        }
    }

    /// Get the amount (0 for wallet creation)
    pub fn amount(&self) -> Decimal {
        match self {
            WalletEvent::WalletCreated { .. } => Decimal::ZERO,
            WalletEvent::WalletFunded { amount, .. } => *amount,
            WalletEvent::TransferCompleted { amount, .. } => *amount,
        }
    }
}

// API Response models

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn error(message: String) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            message: Some(message),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EventResponse {
    pub id: String,
    pub wallet_id: String,
    pub user_id: String,
    pub amount: Decimal,
    pub event_type: String,
    pub created_at: DateTime<Utc>,
}

impl From<TransactionEvent> for EventResponse {
    fn from(event: TransactionEvent) -> Self {
        Self {
            id: event.id,
            wallet_id: event.wallet_id,
            user_id: event.user_id,
            amount: event.amount,
            event_type: event.event_type,
            created_at: event.created_at,
        }
    }
}
