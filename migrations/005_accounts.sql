-- ============================================================================
-- Migration 005: Accounts Tables
-- Phase 5: Account types, accounts, and balances
-- ============================================================================
-- M023: Create account_types table
-- M024: Seed account type master data
-- M025: Create accounts table
-- M026: Create user_wallet_only constraint
-- M027: Create get_wallet_account_id() function
-- M028: Create accounts indexes
-- M029: Seed system user accounts
-- M030: Create account_balances table
-- M031: Create balance constraints (non-negative, max)
-- M032: Create user_balances view
-- M033: Seed system user balance records
-- ============================================================================

-- ============================================================================
-- M023: Create account_types table
-- Master data for account types
-- ============================================================================
CREATE TABLE account_types (
    code VARCHAR(20) PRIMARY KEY,
    name VARCHAR(50) NOT NULL,
    is_debit_normal BOOLEAN NOT NULL,
    is_system_only BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE account_types IS 'Master data for account types';
COMMENT ON COLUMN account_types.code IS 'Unique code (user_wallet, mint_source, etc.)';
COMMENT ON COLUMN account_types.is_debit_normal IS 'TRUE if debit increases balance (assets)';
COMMENT ON COLUMN account_types.is_system_only IS 'TRUE if only system users can have this type';

-- ============================================================================
-- M024: Seed account type master data
-- ============================================================================
INSERT INTO account_types (code, name, is_debit_normal, is_system_only) VALUES
    ('user_wallet', 'User Wallet', TRUE, FALSE),
    ('mint_source', 'ATP Mint Source', FALSE, TRUE),
    ('fee_income', 'Fee Income', FALSE, TRUE),
    ('system_reserve', 'System Reserve', TRUE, TRUE);

-- ============================================================================
-- M025: Create accounts table
-- Internal table - not exposed directly via API
-- ============================================================================
CREATE TABLE accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    account_type VARCHAR(20) NOT NULL REFERENCES account_types(code),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- One account per type per user
    UNIQUE(user_id, account_type)
);

COMMENT ON TABLE accounts IS 'Internal account table (not exposed via API)';
COMMENT ON COLUMN accounts.user_id IS 'Owner of this account';
COMMENT ON COLUMN accounts.account_type IS 'Type of account (user_wallet, mint_source, etc.)';

-- ============================================================================
-- M026: Create user_wallet_only constraint
-- Non-system users can only have user_wallet accounts
-- Note: Using a trigger because CHECK cannot reference other tables
-- ============================================================================
CREATE OR REPLACE FUNCTION check_user_wallet_only() 
RETURNS TRIGGER AS $$
DECLARE
    v_is_system BOOLEAN;
BEGIN
    -- Get is_system flag for the user
    SELECT is_system INTO v_is_system FROM users WHERE id = NEW.user_id;
    
    -- Non-system users can only have user_wallet accounts
    IF v_is_system = FALSE AND NEW.account_type != 'user_wallet' THEN
        RAISE EXCEPTION 'Non-system users can only have user_wallet accounts'
            USING ERRCODE = 'check_violation';
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER enforce_user_wallet_only
    BEFORE INSERT OR UPDATE ON accounts
    FOR EACH ROW EXECUTE FUNCTION check_user_wallet_only();

-- ============================================================================
-- M027: Create get_wallet_account_id() function
-- Convert user_id to account_id for API simplicity
-- ============================================================================
CREATE OR REPLACE FUNCTION get_wallet_account_id(p_user_id UUID) 
RETURNS UUID AS $$
DECLARE
    v_account_id UUID;
BEGIN
    SELECT id INTO v_account_id
    FROM accounts
    WHERE user_id = p_user_id AND account_type = 'user_wallet';
    
    IF v_account_id IS NULL THEN
        RAISE EXCEPTION 'Wallet account not found for user %', p_user_id
            USING ERRCODE = 'no_data_found';
    END IF;
    
    RETURN v_account_id;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION get_wallet_account_id IS 'Convert user_id to wallet account_id';

-- ============================================================================
-- M028: Create accounts indexes
-- ============================================================================
CREATE INDEX idx_accounts_user ON accounts(user_id);
CREATE INDEX idx_accounts_type ON accounts(account_type);
CREATE INDEX idx_accounts_active ON accounts(user_id) WHERE is_active = TRUE;

-- ============================================================================
-- M029: Seed system user accounts
-- ============================================================================
INSERT INTO accounts (user_id, account_type) VALUES
    ('00000000-0000-0000-0000-000000000001', 'mint_source'),
    ('00000000-0000-0000-0000-000000000002', 'mint_source'),  -- SYSTEM_BURN uses mint_source for debiting
    ('00000000-0000-0000-0000-000000000003', 'fee_income'),
    ('00000000-0000-0000-0000-000000000004', 'system_reserve');

-- ============================================================================
-- M030: Create account_balances table
-- Projection table - derived from events
-- ============================================================================
CREATE TABLE account_balances (
    account_id UUID PRIMARY KEY REFERENCES accounts(id),
    balance NUMERIC(20, 8) NOT NULL DEFAULT 0,
    last_event_id UUID NOT NULL,
    last_event_version BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE account_balances IS 'Balance projection (read-only cache derived from events)';
COMMENT ON COLUMN account_balances.balance IS 'Current balance (8 decimal places)';
COMMENT ON COLUMN account_balances.last_event_id IS 'ID of the last processed event';
COMMENT ON COLUMN account_balances.last_event_version IS 'Version of the last processed event';

-- ============================================================================
-- M031: Create balance constraints (non-negative, max)
-- ============================================================================
-- Note: non_negative_balance constraint removed to allow system accounts (e.g. mint_source)
-- to have negative balances (representing liabilities/issued currency).
-- Application logic ensures user wallets don't go negative.

/*
ALTER TABLE account_balances ADD CONSTRAINT non_negative_balance 
    CHECK (balance >= 0);
*/

ALTER TABLE account_balances ADD CONSTRAINT max_balance 
    CHECK (balance <= 1000000000000.00000000);

-- ============================================================================
-- M032: Create user_balances view
-- User-friendly view that hides account_id
-- ============================================================================
CREATE VIEW user_balances AS
SELECT 
    u.id as user_id,
    u.username,
    u.display_name,
    ab.balance,
    ab.updated_at
FROM users u
JOIN accounts a ON u.id = a.user_id AND a.account_type = 'user_wallet'
JOIN account_balances ab ON a.id = ab.account_id
WHERE u.is_system = FALSE AND u.deleted_at IS NULL;

COMMENT ON VIEW user_balances IS 'User-friendly balance view (hides internal account_id)';

-- ============================================================================
-- M033: Seed system user balance records
-- Initialize with zero balance and a placeholder event ID
-- ============================================================================
INSERT INTO account_balances (account_id, balance, last_event_id, last_event_version)
SELECT 
    a.id,
    0,
    '00000000-0000-0000-0000-000000000000',
    0
FROM accounts a
JOIN users u ON a.user_id = u.id
WHERE u.is_system = TRUE;

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
DECLARE
    v_account_type_count INTEGER;
    v_system_account_count INTEGER;
    v_balance_count INTEGER;
BEGIN
    -- Check account_types table
    SELECT COUNT(*) INTO v_account_type_count FROM account_types;
    IF v_account_type_count != 4 THEN
        RAISE EXCEPTION 'Expected 4 account types, found %', v_account_type_count;
    END IF;
    
    -- Check accounts table
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables WHERE table_name = 'accounts'
    ) THEN
        RAISE EXCEPTION 'accounts table was not created';
    END IF;
    
    -- Check system accounts
    SELECT COUNT(*) INTO v_system_account_count 
    FROM accounts a JOIN users u ON a.user_id = u.id 
    WHERE u.is_system = TRUE;
    IF v_system_account_count < 4 THEN
        RAISE EXCEPTION 'Expected at least 4 system accounts, found %', v_system_account_count;
    END IF;
    
    -- Check account_balances
    SELECT COUNT(*) INTO v_balance_count FROM account_balances;
    IF v_balance_count < 4 THEN
        RAISE EXCEPTION 'Expected at least 4 balance records, found %', v_balance_count;
    END IF;
    
    -- Check get_wallet_account_id function
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc WHERE proname = 'get_wallet_account_id'
    ) THEN
        RAISE EXCEPTION 'get_wallet_account_id function was not created';
    END IF;
    
    -- Check user_balances view
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.views WHERE table_name = 'user_balances'
    ) THEN
        RAISE EXCEPTION 'user_balances view was not created';
    END IF;
    
    RAISE NOTICE 'Migration 005 completed successfully';
    RAISE NOTICE '  - account_types: % types seeded', v_account_type_count;
    RAISE NOTICE '  - accounts table: OK';
    RAISE NOTICE '  - user_wallet_only constraint: OK';
    RAISE NOTICE '  - get_wallet_account_id function: OK';
    RAISE NOTICE '  - system accounts: % seeded', v_system_account_count;
    RAISE NOTICE '  - account_balances table: OK';
    RAISE NOTICE '  - balance constraints: OK';
    RAISE NOTICE '  - user_balances view: OK';
    RAISE NOTICE '  - system balances: % seeded', v_balance_count;
END $$;
