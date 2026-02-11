use crate::errors::{WalletError, WalletResult};
use crate::models::{TransactionStatus, TransactionType, Wallet, WalletTransaction};
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Repository for wallet database operations
/// 
/// Design principle: All database logic lives here
/// - Handlers don't know SQL
/// - Repository doesn't know HTTP
/// - Clean separation of concerns
#[derive(Clone)]
pub struct WalletRepository {
    pool: PgPool,
}

impl WalletRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new wallet for a user
    /// 
    /// Business rules:
    /// - Each wallet gets a unique UUID
    /// - Initial balance is 0
    /// - Version starts at 0
    pub async fn create_wallet(&self, user_id: &str) -> WalletResult<Wallet> {
        let wallet_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let wallet = sqlx::query_as::<_, Wallet>(
            r#"
            INSERT INTO wallets (id, user_id, balance, version, created_at, updated_at)
            VALUES ($1, $2, 0, 0, $3, $3)
            RETURNING id, user_id, balance, version, created_at, updated_at
            "#,
        )
        .bind(&wallet_id)
        .bind(user_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        Ok(wallet)
    }

    /// Find a wallet by ID
    pub async fn find_by_id(&self, wallet_id: &str) -> WalletResult<Wallet> {
        let wallet = sqlx::query_as::<_, Wallet>(
            r#"
            SELECT id, user_id, balance, version, created_at, updated_at
            FROM wallets
            WHERE id = $1
            "#,
        )
        .bind(wallet_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| WalletError::WalletNotFound(wallet_id.to_string()))?;

        Ok(wallet)
    }

    /// Find all wallets for a user
    pub async fn find_by_user_id(&self, user_id: &str) -> WalletResult<Vec<Wallet>> {
        let wallets = sqlx::query_as::<_, Wallet>(
            r#"
            SELECT id, user_id, balance, version, created_at, updated_at
            FROM wallets
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(wallets)
    }

    /// Fund a wallet - Add money to wallet balance
    /// 
    /// CRITICAL: This uses optimistic locking!
    /// 
    /// How it works:
    /// 1. Read wallet with current version
    /// 2. Calculate new balance
    /// 3. Update WHERE version matches what we read
    /// 4. If version changed (concurrent update), UPDATE affects 0 rows
    /// 5. Return OptimisticLockError - client should retry
    /// 
    /// Why this matters:
    /// - Prevents lost updates in concurrent scenarios
    /// - No row-level locks needed
    /// - Better performance under contention
    pub async fn fund_wallet(
        &self,
        wallet_id: &str,
        amount: Decimal,
    ) -> WalletResult<(Wallet, WalletTransaction)> {
        // Validate amount
        if amount <= Decimal::ZERO {
            return Err(WalletError::InvalidAmount(
                "Amount must be positive".to_string(),
            ));
        }

        // Start a transaction - all or nothing
        let mut tx = self.pool.begin().await?;

        // Get current wallet state
        let wallet = self.find_by_id_in_tx(&mut tx, wallet_id).await?;
        let new_balance = wallet.balance + amount;
        let new_version = wallet.version + 1;

        // Update with optimistic lock check
        let rows_affected = sqlx::query(
            r#"
            UPDATE wallets
            SET balance = $1, version = $2
            WHERE id = $3 AND version = $4
            "#,
        )
        .bind(new_balance)
        .bind(new_version)
        .bind(wallet_id)
        .bind(wallet.version) // This is the optimistic lock!
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            // Someone else updated this wallet between our read and write
            return Err(WalletError::OptimisticLockError);
        }

        // Record the transaction
        let transaction = self
            .create_transaction_in_tx(
                &mut tx,
                wallet_id,
                amount,
                TransactionType::Fund,
                TransactionStatus::Completed,
                None,
            )
            .await?;

        // Commit the transaction
        tx.commit().await?;

        // Fetch updated wallet
        let updated_wallet = self.find_by_id(wallet_id).await?;

        Ok((updated_wallet, transaction))
    }

    /// Transfer money between wallets
    /// 
    /// This is the most complex operation - it must:
    /// 1. Lock BOTH wallets in a consistent order (prevent deadlock)
    /// 2. Validate sender has enough balance
    /// 3. Update both balances
    /// 4. Create TWO transaction records
    /// 5. All in a single database transaction
    /// 
    /// Deadlock prevention:
    /// - Always lock wallets in ID order (alphabetically)
    /// - If thread A locks wallet-1 then wallet-2
    /// - And thread B locks wallet-1 then wallet-2 (same order)
    /// - No circular wait = no deadlock
    pub async fn transfer(
        &self,
        from_wallet_id: &str,
        to_wallet_id: &str,
        amount: Decimal,
    ) -> WalletResult<(WalletTransaction, WalletTransaction)> {
        // Validate amount
        if amount <= Decimal::ZERO {
            return Err(WalletError::InvalidAmount(
                "Transfer amount must be positive".to_string(),
            ));
        }

        // Can't transfer to yourself
        if from_wallet_id == to_wallet_id {
            return Err(WalletError::InvalidAmount(
                "Cannot transfer to the same wallet".to_string(),
            ));
        }

        // Start transaction
        let mut tx = self.pool.begin().await?;

        // Lock wallets in consistent order to prevent deadlock
        let (first_id, second_id) = if from_wallet_id < to_wallet_id {
            (from_wallet_id, to_wallet_id)
        } else {
            (to_wallet_id, from_wallet_id)
        };

        // Lock both wallets with SELECT ... FOR UPDATE
        // This ensures no one else can modify them until we commit
        let from_wallet = self.lock_wallet_in_tx(&mut tx, first_id).await?;
        let to_wallet = self.lock_wallet_in_tx(&mut tx, second_id).await?;

        // Figure out which is which (we may have locked them in reverse order)
        let (mut from_wallet, mut to_wallet) = if from_wallet.id == from_wallet_id {
            (from_wallet, to_wallet)
        } else {
            (to_wallet, from_wallet)
        };

        // Check sufficient balance
        if from_wallet.balance < amount {
            return Err(WalletError::InsufficientBalance {
                required: amount,
                available: from_wallet.balance,
            });
        }

        // Calculate new balances
        from_wallet.balance -= amount;
        to_wallet.balance += amount;

        // Update both wallets
        sqlx::query(
            r#"
            UPDATE wallets
            SET balance = $1, version = version + 1
            WHERE id = $2
            "#,
        )
        .bind(from_wallet.balance)
        .bind(&from_wallet.id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE wallets
            SET balance = $1, version = version + 1
            WHERE id = $2
            "#,
        )
        .bind(to_wallet.balance)
        .bind(&to_wallet.id)
        .execute(&mut *tx)
        .await?;

        // Create a reference ID to link these two transactions
        let reference_id = Uuid::new_v4().to_string();

        // Record outgoing transaction
        let out_transaction = self
            .create_transaction_in_tx(
                &mut tx,
                &from_wallet.id,
                amount,
                TransactionType::TransferOut,
                TransactionStatus::Completed,
                Some(&reference_id),
            )
            .await?;

        // Record incoming transaction
        let in_transaction = self
            .create_transaction_in_tx(
                &mut tx,
                &to_wallet.id,
                amount,
                TransactionType::TransferIn,
                TransactionStatus::Completed,
                Some(&reference_id),
            )
            .await?;

        // Commit everything
        tx.commit().await?;

        Ok((out_transaction, in_transaction))
    }

    // === Helper methods for working within transactions ===

    /// Find wallet within an existing transaction
    async fn find_by_id_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        wallet_id: &str,
    ) -> WalletResult<Wallet> {
        let wallet = sqlx::query_as::<_, Wallet>(
            r#"
            SELECT id, user_id, balance, version, created_at, updated_at
            FROM wallets
            WHERE id = $1
            "#,
        )
        .bind(wallet_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| WalletError::WalletNotFound(wallet_id.to_string()))?;

        Ok(wallet)
    }

    /// Lock a wallet for update (prevents concurrent modifications)
    async fn lock_wallet_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        wallet_id: &str,
    ) -> WalletResult<Wallet> {
        let wallet = sqlx::query_as::<_, Wallet>(
            r#"
            SELECT id, user_id, balance, version, created_at, updated_at
            FROM wallets
            WHERE id = $1
            FOR UPDATE  -- This is the lock!
            "#,
        )
        .bind(wallet_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| WalletError::WalletNotFound(wallet_id.to_string()))?;

        Ok(wallet)
    }

    /// Create a transaction record within an existing database transaction
    async fn create_transaction_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        wallet_id: &str,
        amount: Decimal,
        transaction_type: TransactionType,
        status: TransactionStatus,
        reference_id: Option<&str>,
    ) -> WalletResult<WalletTransaction> {
        let transaction_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let transaction = sqlx::query_as::<_, WalletTransaction>(
            r#"
            INSERT INTO wallet_transactions (id, wallet_id, amount, type, status, reference_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, wallet_id, amount, type as transaction_type, status, reference_id, created_at
            "#,
        )
        .bind(&transaction_id)
        .bind(wallet_id)
        .bind(amount)
        .bind(transaction_type.to_string())
        .bind(status.to_string())
        .bind(reference_id)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;

        Ok(transaction)
    }
}
