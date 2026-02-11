# Digital Wallet Transaction System

A distributed digital wallet system built with Rust, PostgreSQL, and Kafka demonstrating:
- Event-driven architecture
- ACID transactions with optimistic locking
- Event sourcing and eventual consistency
- Kafka producer/consumer patterns

## Architecture

```
┌─────────────────┐         ┌──────────────┐         ┌─────────────────┐
│ Wallet Service  │────────▶│    Kafka     │────────▶│ History Service │
│   (Port 3000)   │ publish │wallet-events │ consume │   (Port 3001)   │
└────────┬────────┘         └──────────────┘         └────────┬────────┘
         │                                                     │
         │                 ┌──────────────────────┐           │
         └────────────────▶│    PostgreSQL        │◀──────────┘
                           │  (Shared Database)   │
                           └──────────────────────┘
```

## Services

### Wallet Service (Port 3000)
- Create wallets for users
- Fund wallets with money
- Transfer money between wallets
- Publishes events to Kafka
- **Tech:** Axum, SQLx, Rust, PostgreSQL

### History Service (Port 3001)
- Consumes wallet events from Kafka
- Stores event-sourced transaction history
- Provides query APIs for transaction history
- Handles duplicate events (idempotent)
- **Tech:** Axum, SQLx, rdkafka, PostgreSQL

## Tech Stack

- **Language:** Rust (Edition 2021)
- **Web Framework:** Axum 0.7
- **Database:** PostgreSQL 16
- **Message Broker:** Apache Kafka
- **Async Runtime:** Tokio
- **Money Handling:** rust_decimal (no floats!)

## Quick Start

### Prerequisites

- Rust 1.75+
- PostgreSQL 16+
- Apache Kafka + Zookeeper
- Java 17+ (for running Kafka)

### Setup

```bash
# Clone the repository
git clone <your-repo-url>
cd digital-wallet

# Setup infrastructure
cd scripts
./setup-postgres.sh
./setup-kafka.sh

# Start Kafka (PostgreSQL auto-starts)
./start-kafka.sh

# Terminal 1: Run wallet service
cd ../wallet-service
cargo run

# Terminal 2: Run history service
cd ../history-service
cargo run
```

### Test It

```bash
# Create a wallet
curl -X POST http://localhost:3000/wallets \
  -H "Content-Type: application/json" \
  -d '{"user_id": "alice"}'

# Fund the wallet
curl -X POST http://localhost:3000/wallets/{WALLET_ID}/fund \
  -H "Content-Type: application/json" \
  -d '{"amount": "100.00"}'

# Check history (wait 1-2 seconds for event processing)
curl http://localhost:3001/wallets/{WALLET_ID}/history
```

## Project Structure

```
digital-wallet/
├── .gitignore
├── README.md                 # This file
├── docker-compose.yml        # Optional: Docker setup
├── scripts/                  # Setup scripts
│   ├── setup-postgres.sh
│   ├── setup-kafka.sh
│   ├── start-kafka.sh
│   └── stop-kafka.sh
├── wallet-service/           # Main transaction service
│   ├── src/
│   │   ├── main.rs
│   │   ├── models.rs
│   │   ├── repository.rs    # Database operations
│   │   ├── handlers.rs      # HTTP endpoints
│   │   ├── kafka.rs         # Event publishing
│   │   └── errors.rs
│   ├── migrations/           # Database migrations
│   ├── Cargo.toml
│   ├── .env.example
│   └── README.md
└── history-service/          # Event consumer & query service
    ├── src/
    │   ├── main.rs
    │   ├── models.rs
    │   ├── repository.rs
    │   ├── handlers.rs
    │   ├── consumer.rs      # Kafka consumer
    │   └── errors.rs
    ├── migrations/
    ├── Cargo.toml
    ├── .env.example
    └── README.md
```

## Key Features

### 1. Optimistic Locking
Prevents lost updates during concurrent operations:
```rust
UPDATE wallets 
SET balance = new_balance, version = version + 1
WHERE id = wallet_id AND version = current_version;
```

### 2. Deadlock Prevention
Transfers always lock wallets in consistent order:
```rust
let (first, second) = if wallet_a < wallet_b {
    (wallet_a, wallet_b)
} else {
    (wallet_b, wallet_a)
};
```

### 3. Event Sourcing
Complete audit trail of all operations:
```sql
SELECT * FROM transaction_events 
WHERE wallet_id = 'abc-123' 
ORDER BY created_at DESC;
```

### 4. Idempotent Event Processing
Handles Kafka's at-least-once delivery:
```rust
if already_processed(transaction_id) {
    return; // Skip duplicate
}
```

### 5. Decimal Precision
Never use floats for money:
```rust
use rust_decimal::Decimal; // Exact precision
let amount: Decimal = dec!(100.00);
```

## API Documentation

### Wallet Service (Port 3000)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/wallets` | Create a new wallet |
| GET | `/wallets/:id` | Get wallet details |
| GET | `/users/:id/wallets` | List user's wallets |
| POST | `/wallets/:id/fund` | Add money to wallet |
| POST | `/wallets/:id/transfer` | Transfer between wallets |
| GET | `/health` | Health check |

### History Service (Port 3001)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/wallets/:id/history` | Get transaction history |
| GET | `/users/:id/activity` | Get user activity |
| GET | `/health` | Health check |

## Database Schema

### Wallets Table
```sql
CREATE TABLE wallets (
    id VARCHAR(36) PRIMARY KEY,
    user_id VARCHAR(100) NOT NULL,
    balance DECIMAL(19,4) NOT NULL DEFAULT 0,
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);
```

### Transaction Events Table
```sql
CREATE TABLE transaction_events (
    id VARCHAR(36) PRIMARY KEY,
    wallet_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(100) NOT NULL,
    amount DECIMAL(19,4) NOT NULL,
    event_type VARCHAR(30) NOT NULL,
    transaction_id VARCHAR(36),
    created_at TIMESTAMP,
    event_data JSONB NOT NULL
);
```

## Configuration

Both services use `.env` files (create from `.env.example`):

```bash
# Wallet Service (.env)
DATABASE_URL=postgres://wallet_user:wallet_pass@localhost:5432/wallet_db
KAFKA_BROKERS=localhost:9092
KAFKA_TOPIC=wallet-events
PORT=3000

# History Service (.env)
DATABASE_URL=postgres://wallet_user:wallet_pass@localhost:5432/wallet_db
KAFKA_BROKERS=localhost:9092
KAFKA_TOPIC=wallet-events
KAFKA_GROUP_ID=history-service-group
PORT=3001
```

## Development

### Running Tests

```bash
# Wallet service tests
cd wallet-service
cargo test

# History service tests
cd history-service
cargo test
```

### Building for Production

```bash
# Build optimized binaries
cargo build --release

# Binaries will be in:
# wallet-service/target/release/wallet-service
# history-service/target/release/history-service
```

### Database Migrations

```bash
# Run migrations manually
cd wallet-service
sqlx migrate run

cd history-service
sqlx migrate run
```

## Monitoring

### Check Kafka Consumer Lag

```bash
~/kafka/bin/kafka-consumer-groups.sh \
  --bootstrap-server localhost:9092 \
  --describe \
  --group history-service-group
```

### Database Consistency Check

```sql
-- Verify wallet balances match transaction sums
SELECT 
  w.balance as current,
  COALESCE(SUM(
    CASE 
      WHEN wt.type = 'FUND' THEN wt.amount
      WHEN wt.type = 'TRANSFER_IN' THEN wt.amount
      WHEN wt.type = 'TRANSFER_OUT' THEN -wt.amount
    END
  ), 0) as calculated
FROM wallets w
LEFT JOIN wallet_transactions wt ON w.id = wt.wallet_id
GROUP BY w.id, w.balance;
```

## Known Limitations

### Dual-Write Problem
If database commit succeeds but Kafka publish fails:
- Wallet state updated ✅
- Event not published ❌
- History won't be updated

**Solution for production:** Implement outbox pattern or use CDC (Debezium).

### Eventual Consistency
History updates are **eventually consistent**:
- Wallet balance: Immediate
- History: Usually < 1 second lag

This is by design! See documentation for details.

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Learning Resources

- [Rust Async Book](https://rust-lang.github.io/async-book/)
- [SQLx Documentation](https://github.com/launchbadge/sqlx)
- [Kafka Documentation](https://kafka.apache.org/documentation/)
- [Designing Data-Intensive Applications](https://dataintensive.net/)

## License

MIT License - Feel free to use this for learning!

## Acknowledgments

Built as a learning project to understand:
- Distributed systems patterns
- Event-driven architecture
- Rust async programming
- Database transactions and consistency

---