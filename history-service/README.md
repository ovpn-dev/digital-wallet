# History Service

Event-sourced transaction history service that consumes wallet events from Kafka.

## What It Does

1. **Consumes Events**: Listens to `wallet-events` Kafka topic
2. **Stores History**: Saves events in `transaction_events` table
3. **Provides APIs**: Query transaction history by wallet or user
4. **Handles Duplicates**: Idempotent processing (uses transaction_id)

## Architecture

```
Kafka Topic          History Service          Database
(wallet-events)                              (transaction_events)
     │                      │                        │
     │──[event]────────────▶│                        │
     │                      │───[store]─────────────▶│
     │                      │                        │
     │                      │◀──[query]──────────────│
     │                      │                        │
```

## Running

```bash
# Make sure PostgreSQL and Kafka are running
cd history-service

# Build
cargo build

# Run
cargo run
```

Service starts on port **3001** (wallet-service is on 3000).

## APIs

### Get Wallet History
```bash
curl http://localhost:3001/wallets/{wallet_id}/history
```

Returns all events for a specific wallet.

### Get User Activity  
```bash
curl http://localhost:3001/users/{user_id}/activity
```

Returns all events across all wallets owned by a user.

## Key Features

### 1. Idempotency
Uses `transaction_id` to prevent duplicate event processing:
```rust
// Check if already processed
if exists(transaction_id) {
    return; // Skip duplicate
}
```

### 2. Transfer Events
Transfers create TWO events:
- One for sender (TRANSFER_OUT)
- One for receiver (TRANSFER_IN)

### 3. Consumer Group
Multiple instances can run in parallel:
```
Instance 1: Processes partition 0
Instance 2: Processes partition 1
```

### 4. Event Sourcing
The `transaction_events` table is the source of truth for history.
Complete event stored in JSONB for debugging.

## Testing

```bash
# 1. Create some transactions in wallet-service
curl -X POST http://localhost:3000/wallets \
  -H "Content-Type: application/json" \
  -d '{"user_id": "alice"}'

# 2. Fund the wallet
curl -X POST http://localhost:3000/wallets/{wallet_id}/fund \
  -H "Content-Type: application/json" \
  -d '{"amount": "100.00"}'

# 3. Check history (should appear within seconds)
curl http://localhost:3001/wallets/{wallet_id}/history

# 4. Check database
psql -U wallet_user -d wallet_db -h localhost
SELECT * FROM transaction_events ORDER BY created_at DESC LIMIT 5;
```

## How It Works

1. **Wallet Service** creates transaction → publishes event to Kafka
2. **Kafka** stores event in topic (durable)
3. **History Service** consumer reads event from Kafka
4. **Idempotency check**: Already processed?
5. **Store** event in `transaction_events` table
6. **Commit** offset to Kafka (marks as processed)
7. **API** queries directly from `transaction_events` table

## Eventual Consistency

- Wallet balance updates: **IMMEDIATE** (wallet-service)
- History updates: **EVENTUAL** (this service)
- Usually < 1 second lag
- Can be seconds if service is down/restarting

## Error Handling

**Transient Errors** (network, DB connection):
- Log and retry
- Kafka will redeliver message

**Permanent Errors** (malformed JSON):
- Log and skip
- Don't crash entire consumer

**Duplicate Messages**:
- Kafka delivers at-least-once
- Idempotency check handles duplicates
- Same event processed multiple times = same result

## Configuration

Edit `.env`:
```bash
DATABASE_URL=postgres://...     # Same DB as wallet-service
KAFKA_BROKERS=localhost:9092    # Kafka location
KAFKA_TOPIC=wallet-events       # Topic to consume
KAFKA_GROUP_ID=history-service  # Consumer group name
PORT=3001                       # HTTP server port
```

## Troubleshooting

**No events appearing:**
```bash
# Check Kafka has events
~/kafka/bin/kafka-console-consumer.sh \
  --bootstrap-server localhost:9092 \
  --topic wallet-events \
  --from-beginning

# Check service logs
# Should see: "Processing event" messages
```

**Duplicate events:**
- This is NORMAL! Kafka delivers at-least-once
- Check logs for "Event already processed, skipping"
- Idempotency prevents duplicates in database

**Service crashes:**
- Check logs for errors
- Kafka will resume from last committed offset
- No events lost!

## Production Considerations

For production, add:
1. **Metrics**: Prometheus metrics for lag, throughput
2. **Dead Letter Queue**: For permanently failed events
3. **Monitoring**: Alert on consumer lag
4. **Backpressure**: Rate limiting if DB is slow
5. **Graceful Shutdown**: Commit offsets before exit
