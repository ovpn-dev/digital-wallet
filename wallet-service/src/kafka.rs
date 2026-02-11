use crate::errors::{WalletError, WalletResult};
use crate::models::Wallet;
use chrono::{DateTime, Utc};
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Wallet events that get published to Kafka
/// 
/// Design decisions:
/// - Each event is self-contained (has all info needed)
/// - Events are immutable (past tense names)
/// - Include timestamp for event ordering
/// - transaction_id for correlation and idempotency
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
        reference_id: String, // Links the two transaction records
        timestamp: DateTime<Utc>,
    },
}

impl WalletEvent {
    /// Get the event type as a string (useful for logging)
    pub fn event_type(&self) -> &str {
        match self {
            WalletEvent::WalletCreated { .. } => "WALLET_CREATED",
            WalletEvent::WalletFunded { .. } => "WALLET_FUNDED",
            WalletEvent::TransferCompleted { .. } => "TRANSFER_COMPLETED",
        }
    }

    /// Get the primary wallet ID for partitioning
    /// 
    /// Kafka partitions by key - all events for same wallet go to same partition
    /// This ensures ordering per wallet
    pub fn wallet_id(&self) -> &str {
        match self {
            WalletEvent::WalletCreated { wallet_id, .. } => wallet_id,
            WalletEvent::WalletFunded { wallet_id, .. } => wallet_id,
            WalletEvent::TransferCompleted {
                from_wallet_id, ..
            } => from_wallet_id,
        }
    }
}

/// Kafka producer wrapper
/// 
/// Why wrap it?
/// - Hide Kafka complexity from business logic
/// - Provide domain-specific publish methods
/// - Centralize error handling
/// - Make testing easier (can mock this trait)
pub struct KafkaProducer {
    producer: FutureProducer,
    topic: String,
}

impl KafkaProducer {
    /// Create a new Kafka producer
    /// 
    /// Configuration explained:
    /// - bootstrap.servers: Where to find Kafka
    /// - acks=all: Wait for all replicas to acknowledge (durability)
    /// - enable.idempotence=true: Exactly-once semantics within producer
    /// - max.in.flight.requests.per.connection=5: Pipelining for performance
    pub fn new(brokers: &str, topic: String) -> WalletResult<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            // Durability settings
            .set("acks", "all") // Wait for all in-sync replicas
            .set("enable.idempotence", "true") // Prevent duplicates
            // Performance tuning
            .set("compression.type", "snappy")
            .set("batch.size", "16384")
            .set("linger.ms", "10")
            .create()
            .map_err(|e| WalletError::KafkaError(format!("Failed to create producer: {}", e)))?;

        Ok(Self { producer, topic })
    }

    /// Publish an event to Kafka
    /// 
    /// Key points:
    /// - Uses wallet_id as partition key (ordering per wallet)
    /// - Serializes to JSON
    /// - Waits for acknowledgment (up to 5 seconds)
    /// - Returns error if publishing fails
    /// 
    /// Trade-off: This blocks until Kafka confirms
    /// - Pro: We know event was published
    /// - Con: Adds latency to API response
    /// 
    /// Production consideration: For high throughput, you might:
    /// 1. Fire-and-forget with retry logic
    /// 2. Use an outbox pattern (write to DB, separate process publishes)
    /// 3. Accept that events might be lost
    pub async fn publish(&self, event: WalletEvent) -> WalletResult<()> {
        let key = event.wallet_id().to_string();
        let payload = serde_json::to_string(&event).map_err(|e| {
            WalletError::InternalError(format!("Failed to serialize event: {}", e))
        })?;

        tracing::info!(
            event_type = event.event_type(),
            wallet_id = %key,
            "Publishing event to Kafka"
        );

        let record = FutureRecord::to(&self.topic)
            .key(&key) // Partition by wallet_id
            .payload(&payload);

        // Send and wait for acknowledgment
        let delivery_status = self
            .producer
            .send(record, Duration::from_secs(5))
            .await;

        match delivery_status {
            Ok((partition, offset)) => {
                tracing::debug!(
                    partition = partition,
                    offset = offset,
                    "Event published successfully"
                );
                Ok(())
            }
            Err((e, _)) => {
                tracing::error!(error = %e, "Failed to publish event");
                Err(WalletError::KafkaError(format!(
                    "Failed to publish event: {}",
                    e
                )))
            }
        }
    }

    /// Publish wallet created event
    pub async fn publish_wallet_created(&self, wallet: &Wallet) -> WalletResult<()> {
        let event = WalletEvent::WalletCreated {
            wallet_id: wallet.id.clone(),
            user_id: wallet.user_id.clone(),
            timestamp: Utc::now(),
        };

        self.publish(event).await
    }

    /// Publish wallet funded event
    pub async fn publish_wallet_funded(
        &self,
        wallet: &Wallet,
        amount: Decimal,
        transaction_id: String,
    ) -> WalletResult<()> {
        let event = WalletEvent::WalletFunded {
            wallet_id: wallet.id.clone(),
            user_id: wallet.user_id.clone(),
            amount,
            new_balance: wallet.balance,
            transaction_id,
            timestamp: Utc::now(),
        };

        self.publish(event).await
    }

    /// Publish transfer completed event
    pub async fn publish_transfer_completed(
        &self,
        from_wallet_id: String,
        from_user_id: String,
        to_wallet_id: String,
        to_user_id: String,
        amount: Decimal,
        reference_id: String,
    ) -> WalletResult<()> {
        let event = WalletEvent::TransferCompleted {
            from_wallet_id,
            from_user_id,
            to_wallet_id,
            to_user_id,
            amount,
            reference_id,
            timestamp: Utc::now(),
        };

        self.publish(event).await
    }
}

// What happens if Kafka publish fails after DB commit?
// 
// CRITICAL PROBLEM: The database transaction succeeded, but event wasn't published!
// 
// Current approach: Return error to client
// - Wallet state changed
// - History won't be updated
// - User sees error but operation succeeded
// 
// Better approaches (for production):
// 
// 1. OUTBOX PATTERN:
//    - Write event to `outbox` table in same DB transaction
//    - Separate process reads outbox and publishes to Kafka
//    - Delete from outbox after publishing
//    - Pros: Guaranteed delivery, transactional
//    - Cons: More complexity, eventual delivery
// 
// 2. CHANGE DATA CAPTURE (CDC):
//    - Use Debezium to stream DB changes to Kafka
//    - PostgreSQL → Kafka automatically
//    - Pros: Zero code, reliable
//    - Cons: Infrastructure complexity
// 
// 3. TWO-PHASE COMMIT:
//    - Coordinate DB and Kafka transactions
//    - Pros: True atomicity
//    - Cons: Performance impact, rare in practice
// 
// For this learning project: We accept the trade-off
// It demonstrates the classic distributed systems problem!

