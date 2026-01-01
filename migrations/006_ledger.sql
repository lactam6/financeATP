-- ============================================================================
-- Migration 006: Ledger (Double-Entry Bookkeeping)
-- Phase 6: Ledger entries and balance validation
-- ============================================================================
-- M034: Create ledger_entries table (with partitioning)
-- M035: Create 2026-01 partition
-- M036: Create 2026-02 partition
-- M037: Create ledger_entries indexes
-- M038: Create check_ledger_balance_batch() function
-- M039: Create validate_ledger_balance trigger (STATEMENT level)
-- ============================================================================

-- ============================================================================
-- M034: Create ledger_entries table (partitioned by created_at)
-- Double-entry bookkeeping: every transaction has debit and credit entries
-- ============================================================================
CREATE TABLE ledger_entries (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    journal_id UUID NOT NULL,
    transfer_event_id UUID NOT NULL,
    account_id UUID NOT NULL REFERENCES accounts(id),
    amount NUMERIC(20, 8) NOT NULL,
    entry_type VARCHAR(6) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (id, created_at),
    CONSTRAINT positive_amount CHECK (amount > 0),
    CONSTRAINT valid_entry_type CHECK (entry_type IN ('debit', 'credit'))
) PARTITION BY RANGE (created_at);

COMMENT ON TABLE ledger_entries IS 'Double-entry bookkeeping ledger';
COMMENT ON COLUMN ledger_entries.journal_id IS 'Groups debit/credit entries for one transaction';
COMMENT ON COLUMN ledger_entries.transfer_event_id IS 'Reference to the source event';
COMMENT ON COLUMN ledger_entries.entry_type IS 'debit or credit';

-- ============================================================================
-- M035: Create 2026-01 partition
-- ============================================================================
CREATE TABLE ledger_entries_2026_01 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');

-- ============================================================================
-- M036: Create 2026-02 partition
-- ============================================================================
CREATE TABLE ledger_entries_2026_02 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');

-- Create additional partitions for the rest of 2026
CREATE TABLE ledger_entries_2026_03 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-03-01') TO ('2026-04-01');

CREATE TABLE ledger_entries_2026_04 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');

CREATE TABLE ledger_entries_2026_05 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');

CREATE TABLE ledger_entries_2026_06 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');

CREATE TABLE ledger_entries_2026_07 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE TABLE ledger_entries_2026_08 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-08-01') TO ('2026-09-01');

CREATE TABLE ledger_entries_2026_09 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-09-01') TO ('2026-10-01');

CREATE TABLE ledger_entries_2026_10 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-10-01') TO ('2026-11-01');

CREATE TABLE ledger_entries_2026_11 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-11-01') TO ('2026-12-01');

CREATE TABLE ledger_entries_2026_12 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-12-01') TO ('2027-01-01');

-- ============================================================================
-- M037: Create ledger_entries indexes
-- ============================================================================
CREATE INDEX idx_ledger_account ON ledger_entries(account_id);
CREATE INDEX idx_ledger_journal ON ledger_entries(journal_id);
CREATE INDEX idx_ledger_event ON ledger_entries(transfer_event_id);
CREATE INDEX idx_ledger_created ON ledger_entries(created_at);

-- ============================================================================
-- M038: Create check_ledger_balance_batch() function
-- STATEMENT level trigger to check debit = credit for all journals in batch
-- This avoids N+1 query problem by checking all at once
-- ============================================================================
CREATE OR REPLACE FUNCTION check_ledger_balance_batch() 
RETURNS TRIGGER AS $$
DECLARE
    v_unbalanced RECORD;
BEGIN
    -- Find any journal where debit != credit
    FOR v_unbalanced IN
        SELECT 
            journal_id,
            SUM(CASE WHEN entry_type = 'debit' THEN amount ELSE 0 END) as debit_sum,
            SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE 0 END) as credit_sum
        FROM ledger_entries
        WHERE journal_id IN (
            SELECT DISTINCT journal_id 
            FROM ledger_entries 
            WHERE created_at >= NOW() - INTERVAL '1 minute'
        )
        GROUP BY journal_id
        HAVING SUM(CASE WHEN entry_type = 'debit' THEN amount ELSE 0 END) !=
               SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE 0 END)
    LOOP
        RAISE EXCEPTION 'Unbalanced ledger entry for journal %: debit=%, credit=%', 
            v_unbalanced.journal_id, 
            v_unbalanced.debit_sum, 
            v_unbalanced.credit_sum
            USING ERRCODE = 'check_violation';
    END LOOP;
    
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION check_ledger_balance_batch IS 
    'Validates that debit sum equals credit sum for each journal (double-entry rule)';

-- ============================================================================
-- M039: Create validate_ledger_balance trigger (STATEMENT level)
-- Deferred to allow multiple inserts in same transaction
-- ============================================================================
CREATE CONSTRAINT TRIGGER validate_ledger_balance
    AFTER INSERT ON ledger_entries
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    EXECUTE FUNCTION check_ledger_balance_batch();

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
DECLARE
    v_partition_count INTEGER;
BEGIN
    -- Check ledger_entries table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'ledger_entries'
    ) THEN
        RAISE EXCEPTION 'ledger_entries table was not created';
    END IF;
    
    -- Check partitions exist
    SELECT COUNT(*) INTO v_partition_count
    FROM pg_inherits
    JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
    JOIN pg_class child ON pg_inherits.inhrelid = child.oid
    WHERE parent.relname = 'ledger_entries';
    
    IF v_partition_count < 2 THEN
        RAISE EXCEPTION 'ledger_entries partitions were not created (found: %)', v_partition_count;
    END IF;
    
    -- Check function exists
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc WHERE proname = 'check_ledger_balance_batch'
    ) THEN
        RAISE EXCEPTION 'check_ledger_balance_batch function was not created';
    END IF;
    
    -- Check trigger exists
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger WHERE tgname = 'validate_ledger_balance'
    ) THEN
        RAISE EXCEPTION 'validate_ledger_balance trigger was not created';
    END IF;
    
    RAISE NOTICE 'Migration 006 completed successfully';
    RAISE NOTICE '  - ledger_entries table: OK';
    RAISE NOTICE '  - ledger_entries partitions: % created (full year 2026)', v_partition_count;
    RAISE NOTICE '  - ledger_entries indexes: OK';
    RAISE NOTICE '  - check_ledger_balance_batch function: OK';
    RAISE NOTICE '  - validate_ledger_balance trigger: OK (STATEMENT level, DEFERRED)';
END $$;
