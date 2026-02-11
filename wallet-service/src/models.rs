use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Wallet entity - represents a user's digital wallet
/// 
/// Key design decisions:
/// - `balance` is Decimal (never f64!) - prevents floating point errors
/// - `version` enables optimistic locking - prevents lost updates
/// - Uses String for user_id to keep auth separate from wallet concerns
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Wallet {
    pub id: String,
    pub user_id: String,
    pub balance: Decimal,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Transaction record - immutable audit trail
/// 
/// Why separate from events?
/// - This is the source of truth for wallet state changes
/// - Events are for communication, these are for accounting
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct WalletTransaction {
    pub id: String,
    pub wallet_id: String,
    pub amount: Decimal,
    #[serde(rename = "type")]
    pub transaction_type: TransactionType,
    pub status: TransactionStatus,
    pub reference_id: Option<String>, // For correlating transfers
    pub created_at: DateTime<Utc>,
}

/// Transaction type - what kind of operation happened
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar")]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionType {
    #[serde(rename = "FUND")]
    Fund,           // Adding money to wallet
    
    #[serde(rename = "TRANSFER_OUT")]
    TransferOut,    // Sending money
    
    #[serde(rename = "TRANSFER_IN")]
    TransferIn,     // Receiving money
}

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionType::Fund => write!(f, "FUND"),
            TransactionType::TransferOut => write!(f, "TRANSFER_OUT"),
            TransactionType::TransferIn => write!(f, "TRANSFER_IN"),
        }
    }
}

/// Transaction status - did it work or not?
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar")]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionStatus {
    #[serde(rename = "COMPLETED")]
    Completed,
    
    #[serde(rename = "FAILED")]
    Failed,
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::Completed => write!(f, "COMPLETED"),
            TransactionStatus::Failed => write!(f, "FAILED"),
        }
    }
}

// === API Request/Response Models ===

/// Request to create a new wallet
#[derive(Debug, Deserialize)]
pub struct CreateWalletRequest {
    pub user_id: String,
}

/// Request to fund a wallet
#[derive(Debug, Deserialize)]
pub struct FundWalletRequest {
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

/// Request to transfer money between wallets
#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub to_wallet_id: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

/// Generic API response
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

/// Response for wallet operations
#[derive(Debug, Serialize)]
pub struct WalletResponse {
    pub id: String,
    pub user_id: String,
    pub balance: Decimal,
    pub created_at: DateTime<Utc>,
}

impl From<Wallet> for WalletResponse {
    fn from(wallet: Wallet) -> Self {
        Self {
            id: wallet.id,
            user_id: wallet.user_id,
            balance: wallet.balance,
            created_at: wallet.created_at,
        }
    }
}

/// Response for transaction operations
#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub transaction_id: String,
    pub wallet_id: String,
    pub amount: Decimal,
    #[serde(rename = "type")]
    pub transaction_type: TransactionType,
    pub status: TransactionStatus,
    pub created_at: DateTime<Utc>,
}

impl From<WalletTransaction> for TransactionResponse {
    fn from(txn: WalletTransaction) -> Self {
        Self {
            transaction_id: txn.id,
            wallet_id: txn.wallet_id,
            amount: txn.amount,
            transaction_type: txn.transaction_type,
            status: txn.status,
            created_at: txn.created_at,
        }
    }
}
