-- Create transaction_events table
-- This stores the event-sourced history of all wallet operations
-- 
-- Key design decisions:
-- 1. transaction_id for idempotency (prevents duplicate processing)
-- 2. JSONB event_data for full event storage (debugging + audit)
-- 3. Indexes on wallet_id and user_id for fast queries

CREATE TABLE IF NOT EXISTS transaction_events (
    id VARCHAR(36) PRIMARY KEY,
    wallet_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(100) NOT NULL,
    amount DECIMAL(19,4) NOT NULL,
    event_type VARCHAR(30) NOT NULL,
    transaction_id VARCHAR(36),  -- For idempotency checking
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    event_data JSONB NOT NULL
);

-- Index for querying by wallet
CREATE INDEX idx_transaction_events_wallet_id ON transaction_events(wallet_id);

-- Index for querying by user
CREATE INDEX idx_transaction_events_user_id ON transaction_events(user_id);

-- Index for idempotency checks (must be fast!)
CREATE UNIQUE INDEX idx_transaction_events_transaction_id 
    ON transaction_events(transaction_id) 
    WHERE transaction_id IS NOT NULL;

-- Index for time-based queries
CREATE INDEX idx_transaction_events_created_at ON transaction_events(created_at DESC);

-- Composite index for common query pattern (user + time)
CREATE INDEX idx_transaction_events_user_created 
    ON transaction_events(user_id, created_at DESC);
