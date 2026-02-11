use crate::errors::HistoryResult;
use crate::models::WalletEvent;
use crate::repository::EventRepository;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use tokio::time::{sleep, Duration};

/// Kafka consumer for wallet events
/// 
/// Key concepts:
/// - Consumer Group: Multiple instances can share the workload
/// - Auto-commit: Automatically tracks which messages we've processed
/// - Partition assignment: Kafka assigns partitions to consumers
pub struct EventConsumer {
    consumer: StreamConsumer,
    repository: EventRepository,
}

impl EventConsumer {
    /// Create a new Kafka consumer
    /// 
    /// Configuration:
    /// - group.id: Consumer group name (for parallel processing)
    /// - auto.offset.reset: Where to start if no offset exists
    /// - enable.auto.commit: Automatically save progress
    pub fn new(
        brokers: &str,
        group_id: &str,
        topic: &str,
        repository: EventRepository,
    ) -> HistoryResult<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("auto.offset.reset", "earliest") // Start from beginning if no offset
            .set("enable.auto.commit", "true") // Auto-commit offsets
            .set("auto.commit.interval.ms", "5000") // Commit every 5 seconds
            .set("session.timeout.ms", "30000")
            .set("enable.partition.eof", "false")
            .create()
            .map_err(|e| crate::errors::HistoryError::KafkaError(format!("Failed to create consumer: {}", e)))?;

        consumer
            .subscribe(&[topic])
            .map_err(|e| crate::errors::HistoryError::KafkaError(format!("Failed to subscribe: {}", e)))?;

        Ok(Self {
            consumer,
            repository,
        })
    }

    /// Start consuming events - this runs forever
    /// 
    /// Flow:
    /// 1. Poll Kafka for new messages
    /// 2. Deserialize JSON to WalletEvent
    /// 3. Store in database (with idempotency check)
    /// 4. Auto-commit happens in background
    /// 
    /// Error handling:
    /// - Deserialization errors: Log and skip (don't crash)
    /// - Database errors: Log and retry (transient failures)
    /// - Fatal errors: Return and let service restart
    pub async fn start(self) -> HistoryResult<()> {
        tracing::info!("Starting Kafka consumer...");

        loop {
            match self.consumer.recv().await {
                Ok(message) => {
                    if let Some(payload) = message.payload() {
                        match self.process_message(payload).await {
                            Ok(_) => {
                                tracing::debug!("Message processed successfully");
                            }
                            Err(e) => {
                                tracing::error!(
                                    error = %e,
                                    "Failed to process message, will retry on next poll"
                                );
                                // Don't crash - log and continue
                                // Message will be reprocessed if we haven't committed yet
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Kafka error");
                    // Sleep briefly before retrying
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// Process a single message from Kafka
    async fn process_message(&self, payload: &[u8]) -> HistoryResult<()> {
        // Deserialize JSON to WalletEvent
        let event: WalletEvent = serde_json::from_slice(payload)
            .map_err(|e| {
                tracing::warn!(
                    error = %e,
                    payload = ?String::from_utf8_lossy(payload),
                    "Failed to deserialize event"
                );
                crate::errors::HistoryError::SerializationError(e.to_string())
            })?;

        tracing::info!(
            event_type = %event.event_type(),
            wallet_id = %event.wallet_id(),
            "Processing event"
        );

        // Store in database based on event type
        match &event {
            WalletEvent::TransferCompleted { .. } => {
                // Transfers create TWO events (sender + receiver)
                let events = self.repository.store_transfer_events(&event).await?;
                tracing::info!(
                    event_count = events.len(),
                    "Transfer events stored"
                );
            }
            _ => {
                // Other events create ONE event
                if let Some(stored_event) = self.repository.store_event(&event).await? {
                    tracing::info!(
                        event_id = %stored_event.id,
                        "Event stored"
                    );
                } else {
                    tracing::debug!("Event already processed (duplicate)");
                }
            }
        }

        Ok(())
    }
}

// Why consumer group?
// 
// Multiple History Service instances can run in parallel:
// 
// Instance 1: Processes partition 0
// Instance 2: Processes partition 1  
// Instance 3: Processes partition 2
// 
// Benefits:
// - Parallel processing (faster)
// - High availability (if one dies, others continue)
// - Automatic rebalancing (Kafka reassigns partitions)
// 
// Trade-off:
// - Events for same wallet always go to same partition (ordering preserved)
// - But different wallets might be processed out of global order (that's OK!)
