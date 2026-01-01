-- ============================================================================
-- Migration 001: Database Foundation
-- Phase 1: Database Infrastructure
-- ============================================================================
-- M001: Enable uuid-ossp extension
-- M002: Enable pgcrypto extension
-- M003: Create prevent_event_modification() trigger function
-- M004: Immutable table trigger (function definition only, applied per table)
-- ============================================================================

-- ============================================================================
-- M001: Enable uuid-ossp extension
-- Required for UUID generation
-- ============================================================================
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ============================================================================
-- M002: Enable pgcrypto extension
-- Required for SHA-256 hashing and encryption
-- ============================================================================
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- M003: Create prevent_event_modification() trigger function
-- Prevents UPDATE/DELETE on immutable tables (events, audit_logs)
-- ============================================================================
CREATE OR REPLACE FUNCTION prevent_event_modification() 
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'DELETE is not allowed on % table. This table is immutable.', TG_TABLE_NAME
            USING ERRCODE = 'restrict_violation';
    ELSIF TG_OP = 'UPDATE' THEN
        RAISE EXCEPTION 'UPDATE is not allowed on % table. This table is immutable.', TG_TABLE_NAME
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- M004: Immutable table trigger preparation
-- Note: Actual trigger application is done when creating each table
-- ============================================================================
-- Usage example:
-- CREATE TRIGGER no_modify_events
--     BEFORE UPDATE OR DELETE ON events
--     FOR EACH ROW EXECUTE FUNCTION prevent_event_modification();
--
-- CREATE TRIGGER no_modify_audit
--     BEFORE UPDATE OR DELETE ON audit_logs
--     FOR EACH ROW EXECUTE FUNCTION prevent_event_modification();

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
BEGIN
    -- Check uuid-ossp
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'uuid-ossp') THEN
        RAISE EXCEPTION 'uuid-ossp extension is not installed';
    END IF;
    
    -- Check pgcrypto
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pgcrypto') THEN
        RAISE EXCEPTION 'pgcrypto extension is not installed';
    END IF;
    
    -- Check prevent_event_modification function
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc 
        WHERE proname = 'prevent_event_modification'
    ) THEN
        RAISE EXCEPTION 'prevent_event_modification function is not created';
    END IF;
    
    RAISE NOTICE 'Migration 001 completed successfully';
    RAISE NOTICE '  - uuid-ossp extension: OK';
    RAISE NOTICE '  - pgcrypto extension: OK';
    RAISE NOTICE '  - prevent_event_modification function: OK';
END $$;
