use crate::errors::{HistoryError, HistoryResult};
use crate::models::{TransactionEvent, WalletEvent};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

/// Repository for transaction event operations
#[derive(Clone)]
pub struct EventRepository {
    pool: PgPool,
}

impl EventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Store an event from Kafka
    /// 
    /// CRITICAL: This must be idempotent!
    /// - Uses transaction_id to prevent duplicates
    /// - If event with same transaction_id exists, skip it
    /// 
    /// Why? Kafka delivers at-least-once, so we might see the same event multiple times
    pub async fn store_event(&self, event: &WalletEvent) -> HistoryResult<Option<TransactionEvent>> {
        let event_id = Uuid::new_v4().to_string();
        let wallet_id = event.wallet_id().to_string();
        let user_id = event.user_id().to_string();
        let amount = event.amount();
        let event_type = event.event_type().to_string();
        let transaction_id = event.transaction_id();
        
        // Serialize full event as JSON for debugging
        let event_data = serde_json::to_value(event)
            .map_err(|e| HistoryError::SerializationError(e.to_string()))?;

        // Check if we've already processed this event (idempotency check)
        if let Some(ref txn_id) = transaction_id {
            let exists = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM transaction_events 
                    WHERE transaction_id = $1
                )
                "#
            )
            .bind(txn_id)
            .fetch_one(&self.pool)
            .await?;

            if exists {
                tracing::info!(
                    transaction_id = %txn_id,
                    event_type = %event_type,
                    "Event already processed, skipping (idempotent)"
                );
                return Ok(None); // Already processed
            }
        }

        // Store the event
        let stored_event = sqlx::query_as::<_, TransactionEvent>(
            r#"
            INSERT INTO transaction_events 
                (id, wallet_id, user_id, amount, event_type, transaction_id, event_data, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
            RETURNING id, wallet_id, user_id, amount, event_type, transaction_id, created_at, event_data
            "#
        )
        .bind(&event_id)
        .bind(&wallet_id)
        .bind(&user_id)
        .bind(amount)
        .bind(&event_type)
        .bind(transaction_id.as_ref())
        .bind(&event_data)
        .fetch_one(&self.pool)
        .await?;

        tracing::info!(
            event_id = %event_id,
            wallet_id = %wallet_id,
            event_type = %event_type,
            "Event stored successfully"
        );

        Ok(Some(stored_event))
    }

    /// Handle TRANSFER_COMPLETED event specially
    /// 
    /// Transfers affect TWO wallets, so we create TWO events:
    /// 1. Outgoing event for sender
    /// 2. Incoming event for receiver
    pub async fn store_transfer_events(&self, event: &WalletEvent) -> HistoryResult<Vec<TransactionEvent>> {
        if let WalletEvent::TransferCompleted {
            from_wallet_id,
            from_user_id,
            to_wallet_id,
            to_user_id,
            amount,
            reference_id,
            timestamp,
        } = event
        {
            let event_data = serde_json::to_value(event)
                .map_err(|e| HistoryError::SerializationError(e.to_string()))?;

            // Check if we've already processed this transfer
            let exists = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM transaction_events 
                    WHERE transaction_id = $1
                )
                "#
            )
            .bind(reference_id)
            .fetch_one(&self.pool)
            .await?;

            if exists {
                tracing::info!(
                    reference_id = %reference_id,
                    "Transfer already processed, skipping"
                );
                return Ok(vec![]);
            }

            let mut events = Vec::new();

            // Event 1: Outgoing from sender
            let out_event_id = Uuid::new_v4().to_string();
            let out_event = sqlx::query_as::<_, TransactionEvent>(
                r#"
                INSERT INTO transaction_events 
                    (id, wallet_id, user_id, amount, event_type, transaction_id, event_data, created_at)
                VALUES ($1, $2, $3, $4, 'TRANSFER_OUT', $5, $6, $7)
                RETURNING id, wallet_id, user_id, amount, event_type, transaction_id, created_at, event_data
                "#
            )
            .bind(&out_event_id)
            .bind(from_wallet_id)
            .bind(from_user_id)
            .bind(amount)
            .bind(reference_id)
            .bind(&event_data)
            .bind(timestamp)
            .fetch_one(&self.pool)
            .await?;

            events.push(out_event);

            // Event 2: Incoming to receiver
            let in_event_id = Uuid::new_v4().to_string();
            let in_event = sqlx::query_as::<_, TransactionEvent>(
                r#"
                INSERT INTO transaction_events 
                    (id, wallet_id, user_id, amount, event_type, transaction_id, event_data, created_at)
                VALUES ($1, $2, $3, $4, 'TRANSFER_IN', $5, $6, $7)
                RETURNING id, wallet_id, user_id, amount, event_type, transaction_id, created_at, event_data
                "#
            )
            .bind(&in_event_id)
            .bind(to_wallet_id)
            .bind(to_user_id)
            .bind(amount)
            .bind(reference_id)
            .bind(&event_data)
            .bind(timestamp)
            .fetch_one(&self.pool)
            .await?;

            events.push(in_event);

            tracing::info!(
                reference_id = %reference_id,
                from_wallet = %from_wallet_id,
                to_wallet = %to_wallet_id,
                "Transfer events stored"
            );

            Ok(events)
        } else {
            Err(HistoryError::InternalError(
                "Expected TransferCompleted event".to_string(),
            ))
        }
    }

    /// Get all events for a specific wallet
    pub async fn get_wallet_history(&self, wallet_id: &str) -> HistoryResult<Vec<TransactionEvent>> {
        let events = sqlx::query_as::<_, TransactionEvent>(
            r#"
            SELECT id, wallet_id, user_id, amount, event_type, transaction_id, created_at, event_data
            FROM transaction_events
            WHERE wallet_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(wallet_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    /// Get all events for a specific user (across all their wallets)
    pub async fn get_user_activity(&self, user_id: &str) -> HistoryResult<Vec<TransactionEvent>> {
        let events = sqlx::query_as::<_, TransactionEvent>(
            r#"
            SELECT id, wallet_id, user_id, amount, event_type, transaction_id, created_at, event_data
            FROM transaction_events
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }
}
