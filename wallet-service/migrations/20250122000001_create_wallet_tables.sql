-- Create wallets table
-- Key features:
-- 1. DECIMAL(19,4) for balance - exact precision for money
-- 2. version column for optimistic locking
-- 3. Indexed on user_id for quick lookups
-- 4. Timestamps for auditing

CREATE TABLE IF NOT EXISTS wallets (
    id VARCHAR(36) PRIMARY KEY,
    user_id VARCHAR(100) NOT NULL,
    balance DECIMAL(19,4) NOT NULL DEFAULT 0 CHECK (balance >= 0),
    version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for finding wallets by user
CREATE INDEX idx_wallets_user_id ON wallets(user_id);

-- Create wallet_transactions table
-- This is the immutable audit trail of all wallet operations
-- Each row represents a single transaction affecting one wallet

CREATE TABLE IF NOT EXISTS wallet_transactions (
    id VARCHAR(36) PRIMARY KEY,
    wallet_id VARCHAR(36) NOT NULL,
    amount DECIMAL(19,4) NOT NULL,
    type VARCHAR(20) NOT NULL CHECK (type IN ('FUND', 'TRANSFER_OUT', 'TRANSFER_IN')),
    status VARCHAR(20) NOT NULL CHECK (status IN ('COMPLETED', 'FAILED')),
    reference_id VARCHAR(36), -- For linking related transactions (transfers)
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (wallet_id) REFERENCES wallets(id) ON DELETE CASCADE
);

-- Index for querying transaction history by wallet
CREATE INDEX idx_wallet_transactions_wallet_id ON wallet_transactions(wallet_id);

-- Index for finding related transactions
CREATE INDEX idx_wallet_transactions_reference_id ON wallet_transactions(reference_id) WHERE reference_id IS NOT NULL;

-- Trigger to update the updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_wallets_updated_at 
    BEFORE UPDATE ON wallets 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();
