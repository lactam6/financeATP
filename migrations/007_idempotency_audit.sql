-- ============================================================================
-- Migration 007: Idempotency and Audit Logs
-- Phase 7: Idempotency keys and audit trail with hash chain
-- ============================================================================
-- M040: Create idempotency_keys table
-- M041: Create idempotency_keys indexes
-- M042: Create reset_stale_idempotency_keys() function
-- M043: Create audit_logs table
-- M044: Create audit_logs indexes
-- M045: Create calculate_audit_hash() function (with exclusive lock)
-- M046: Create hash_audit_log trigger
-- M047: Apply immutable trigger to audit_logs
-- ============================================================================

-- ============================================================================
-- M040: Create idempotency_keys table
-- Prevents duplicate request processing
-- ============================================================================
CREATE TABLE idempotency_keys (
    key UUID PRIMARY KEY,
    request_hash VARCHAR(64) NOT NULL,
    event_id UUID,
    response_status INTEGER,
    response_body JSONB,
    processing_status VARCHAR(20) NOT NULL DEFAULT 'pending',
    processing_started_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '24 hours'
);

COMMENT ON TABLE idempotency_keys IS 'Idempotency key storage to prevent duplicate request processing';
COMMENT ON COLUMN idempotency_keys.key IS 'UUID provided by client in Idempotency-Key header';
COMMENT ON COLUMN idempotency_keys.request_hash IS 'SHA-256 hash of request body for conflict detection';
COMMENT ON COLUMN idempotency_keys.processing_status IS 'pending, processing, completed, or failed';
COMMENT ON COLUMN idempotency_keys.processing_started_at IS 'When processing started (for timeout detection)';

-- ============================================================================
-- M041: Create idempotency_keys indexes
-- ============================================================================
CREATE INDEX idx_idempotency_expires ON idempotency_keys(expires_at);
CREATE INDEX idx_idempotency_processing ON idempotency_keys(processing_status, processing_started_at) 
    WHERE processing_status = 'processing';
CREATE INDEX idx_idempotency_status ON idempotency_keys(processing_status);

-- ============================================================================
-- M042: Create reset_stale_idempotency_keys() function
-- Resets keys stuck in 'processing' state (server crash recovery)
-- Should be called every 1 minute
-- ============================================================================
CREATE OR REPLACE FUNCTION reset_stale_idempotency_keys() 
RETURNS INTEGER AS $$
DECLARE
    v_affected INTEGER;
BEGIN
    UPDATE idempotency_keys
    SET processing_status = 'failed'
    WHERE processing_status = 'processing'
      AND processing_started_at < NOW() - INTERVAL '5 minutes';
    
    GET DIAGNOSTICS v_affected = ROW_COUNT;
    RETURN v_affected;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION reset_stale_idempotency_keys IS 
    'Reset idempotency keys stuck in processing state. Returns count of reset keys.';

-- ============================================================================
-- M043: Create audit_logs table
-- Immutable audit trail with hash chain for tamper detection
-- ============================================================================
CREATE TABLE audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sequence_number BIGSERIAL NOT NULL,
    api_key_id UUID REFERENCES api_keys(id),
    request_user_id UUID,
    correlation_id UUID,
    action VARCHAR(50) NOT NULL,
    resource_type VARCHAR(50),
    resource_id UUID,
    before_state JSONB,
    after_state JSONB,
    changed_fields TEXT[],
    client_ip INET,
    previous_hash VARCHAR(64) NOT NULL,
    current_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT unique_sequence UNIQUE (sequence_number)
);

COMMENT ON TABLE audit_logs IS 'Immutable audit trail with hash chain for tamper detection';
COMMENT ON COLUMN audit_logs.sequence_number IS 'Sequential number for ordering and hash chain';
COMMENT ON COLUMN audit_logs.action IS 'Action performed (user.create, transfer.execute, etc.)';
COMMENT ON COLUMN audit_logs.previous_hash IS 'Hash of previous audit log entry';
COMMENT ON COLUMN audit_logs.current_hash IS 'Hash of this entry (includes previous_hash)';

-- ============================================================================
-- M044: Create audit_logs indexes
-- ============================================================================
CREATE INDEX idx_audit_user ON audit_logs(request_user_id, created_at);
CREATE INDEX idx_audit_action ON audit_logs(action, created_at);
CREATE INDEX idx_audit_correlation ON audit_logs(correlation_id);
CREATE INDEX idx_audit_resource ON audit_logs(resource_type, resource_id);
CREATE INDEX idx_audit_sequence ON audit_logs(sequence_number);

-- ============================================================================
-- M045: Create calculate_audit_hash() function
-- Uses pg_advisory_xact_lock to prevent race conditions in hash chain
-- ============================================================================
CREATE OR REPLACE FUNCTION calculate_audit_hash() 
RETURNS TRIGGER AS $$
DECLARE
    v_prev_hash VARCHAR(64);
    v_hash_input TEXT;
BEGIN
    -- Exclusive lock to prevent race condition in hash chain
    PERFORM pg_advisory_xact_lock(hashtext('audit_logs_chain'));
    
    -- Get previous hash (by sequence number)
    SELECT current_hash INTO v_prev_hash 
    FROM audit_logs 
    ORDER BY sequence_number DESC
    LIMIT 1;
    
    -- Use zero hash if this is the first entry
    NEW.previous_hash := COALESCE(
        v_prev_hash, 
        '0000000000000000000000000000000000000000000000000000000000000000'
    );
    
    -- Build hash input string
    v_hash_input := NEW.id::text || 
                    NEW.sequence_number::text ||
                    NEW.action || 
                    COALESCE(NEW.request_user_id::text, '') ||
                    COALESCE(NEW.resource_type, '') ||
                    COALESCE(NEW.resource_id::text, '') ||
                    COALESCE(NEW.before_state::text, '') ||
                    COALESCE(NEW.after_state::text, '') ||
                    NEW.previous_hash;
    
    -- Calculate SHA-256 hash
    NEW.current_hash := encode(sha256(v_hash_input::bytea), 'hex');
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION calculate_audit_hash IS 
    'Calculates hash chain for audit logs. Uses advisory lock to prevent race conditions.';

-- ============================================================================
-- M046: Create hash_audit_log trigger
-- Automatically calculates hash before insert
-- ============================================================================
CREATE TRIGGER hash_audit_log
    BEFORE INSERT ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION calculate_audit_hash();

-- ============================================================================
-- M047: Apply immutable trigger to audit_logs
-- Prevents UPDATE and DELETE operations
-- ============================================================================
CREATE TRIGGER no_modify_audit
    BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION prevent_event_modification();

-- ============================================================================
-- Helper function: Verify audit log chain integrity
-- ============================================================================
CREATE OR REPLACE FUNCTION verify_audit_chain() 
RETURNS TABLE(
    id UUID,
    sequence_number BIGINT,
    is_valid BOOLEAN,
    expected_hash VARCHAR(64),
    actual_hash VARCHAR(64)
) AS $$
DECLARE
    v_prev_hash VARCHAR(64) := '0000000000000000000000000000000000000000000000000000000000000000';
    v_record RECORD;
    v_expected_hash VARCHAR(64);
    v_hash_input TEXT;
BEGIN
    FOR v_record IN 
        SELECT * FROM audit_logs ORDER BY sequence_number ASC
    LOOP
        -- Build hash input
        v_hash_input := v_record.id::text || 
                        v_record.sequence_number::text ||
                        v_record.action || 
                        COALESCE(v_record.request_user_id::text, '') ||
                        COALESCE(v_record.resource_type, '') ||
                        COALESCE(v_record.resource_id::text, '') ||
                        COALESCE(v_record.before_state::text, '') ||
                        COALESCE(v_record.after_state::text, '') ||
                        v_prev_hash;
        
        v_expected_hash := encode(sha256(v_hash_input::bytea), 'hex');
        
        id := v_record.id;
        sequence_number := v_record.sequence_number;
        is_valid := (v_expected_hash = v_record.current_hash AND v_prev_hash = v_record.previous_hash);
        expected_hash := v_expected_hash;
        actual_hash := v_record.current_hash;
        
        RETURN NEXT;
        
        v_prev_hash := v_record.current_hash;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION verify_audit_chain IS 
    'Verifies integrity of audit log hash chain. Returns invalid entries if tampered.';

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
BEGIN
    -- Check idempotency_keys table
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables WHERE table_name = 'idempotency_keys'
    ) THEN
        RAISE EXCEPTION 'idempotency_keys table was not created';
    END IF;
    
    -- Check audit_logs table
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables WHERE table_name = 'audit_logs'
    ) THEN
        RAISE EXCEPTION 'audit_logs table was not created';
    END IF;
    
    -- Check functions
    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'reset_stale_idempotency_keys') THEN
        RAISE EXCEPTION 'reset_stale_idempotency_keys function was not created';
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'calculate_audit_hash') THEN
        RAISE EXCEPTION 'calculate_audit_hash function was not created';
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'verify_audit_chain') THEN
        RAISE EXCEPTION 'verify_audit_chain function was not created';
    END IF;
    
    -- Check triggers
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'hash_audit_log') THEN
        RAISE EXCEPTION 'hash_audit_log trigger was not created';
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'no_modify_audit') THEN
        RAISE EXCEPTION 'no_modify_audit trigger was not created';
    END IF;
    
    RAISE NOTICE 'Migration 007 completed successfully';
    RAISE NOTICE '  - idempotency_keys table: OK';
    RAISE NOTICE '  - idempotency_keys indexes: OK';
    RAISE NOTICE '  - reset_stale_idempotency_keys function: OK';
    RAISE NOTICE '  - audit_logs table: OK';
    RAISE NOTICE '  - audit_logs indexes: OK';
    RAISE NOTICE '  - calculate_audit_hash function: OK (with advisory lock)';
    RAISE NOTICE '  - hash_audit_log trigger: OK';
    RAISE NOTICE '  - no_modify_audit trigger: OK';
    RAISE NOTICE '  - verify_audit_chain function: OK';
END $$;
