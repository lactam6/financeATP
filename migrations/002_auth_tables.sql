-- ============================================================================
-- Migration 002: Authentication & Authorization Tables
-- Phase 2: API Keys and Rate Limiting
-- ============================================================================
-- M005: Create api_keys table
-- M006: Create api_keys indexes
-- M007: Seed initial API key (for development)
-- M008: Create rate_limit_buckets table
-- M009: Create check_and_increment_rate_limit() function
-- M010: Create Rate Limit bucket cleanup function
-- ============================================================================

-- ============================================================================
-- M005: Create api_keys table
-- Service-to-service authentication
-- ============================================================================
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    key_prefix VARCHAR(32) NOT NULL,
    key_hash VARCHAR(64) NOT NULL,
    permissions TEXT[] NOT NULL,
    allowed_ips INET[],
    rate_limit_per_minute INTEGER DEFAULT 1000,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ
);

COMMENT ON TABLE api_keys IS 'API keys for service-to-service authentication';
COMMENT ON COLUMN api_keys.key_prefix IS 'First 8-12 chars of the key (e.g., sk_live_)';
COMMENT ON COLUMN api_keys.key_hash IS 'SHA-256 hash of the remaining key portion';
COMMENT ON COLUMN api_keys.permissions IS 'Array of permissions (e.g., read:accounts, write:transfers)';
COMMENT ON COLUMN api_keys.allowed_ips IS 'Optional IP whitelist';

-- ============================================================================
-- M006: Create api_keys indexes
-- ============================================================================
CREATE INDEX idx_api_keys_active ON api_keys(key_prefix) WHERE is_active = TRUE;
CREATE INDEX idx_api_keys_expires ON api_keys(expires_at) WHERE expires_at IS NOT NULL;

-- ============================================================================
-- M007: Seed initial API key (for development)
-- Key: sk_dev_test1234567890abcdef
-- This is for development only - replace in production!
-- ============================================================================
INSERT INTO api_keys (
    id,
    name,
    key_prefix,
    key_hash,
    permissions,
    rate_limit_per_minute
) VALUES (
    'a0000000-0000-0000-0000-000000000001',
    'Development API Key',
    'sk_dev_',
    encode(sha256('test1234567890abcdef'::bytea), 'hex'),
    ARRAY['read:users', 'write:users', 'read:accounts', 'write:transfers', 'admin:mint', 'admin:burn', 'admin:events', 'admin:api-keys'],
    1000
);

-- ============================================================================
-- M008: Create rate_limit_buckets table
-- Sliding window rate limiting
-- ============================================================================
CREATE TABLE rate_limit_buckets (
    api_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    window_start TIMESTAMPTZ NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (api_key_id, window_start)
);

COMMENT ON TABLE rate_limit_buckets IS 'Rate limiting buckets per API key per minute';
COMMENT ON COLUMN rate_limit_buckets.window_start IS 'Start of the 1-minute window (truncated to minute)';
COMMENT ON COLUMN rate_limit_buckets.request_count IS 'Number of requests in this window';

-- ============================================================================
-- M008 continued: Create rate_limit_buckets indexes
-- ============================================================================
CREATE INDEX idx_rate_limit_expires ON rate_limit_buckets(window_start);
CREATE INDEX idx_rate_limit_key ON rate_limit_buckets(api_key_id);

-- ============================================================================
-- M009: Create check_and_increment_rate_limit() function
-- Returns TRUE if request is allowed, FALSE if rate limit exceeded
-- ============================================================================
CREATE OR REPLACE FUNCTION check_and_increment_rate_limit(
    p_api_key_id UUID,
    p_limit INTEGER
) RETURNS BOOLEAN AS $$
DECLARE
    v_window TIMESTAMPTZ;
    v_count INTEGER;
BEGIN
    -- Get current minute window
    v_window := date_trunc('minute', NOW());
    
    -- Upsert: Insert or increment counter
    INSERT INTO rate_limit_buckets (api_key_id, window_start, request_count)
    VALUES (p_api_key_id, v_window, 1)
    ON CONFLICT (api_key_id, window_start) 
    DO UPDATE SET request_count = rate_limit_buckets.request_count + 1
    RETURNING request_count INTO v_count;
    
    -- Return TRUE if under limit, FALSE if exceeded
    RETURN v_count <= p_limit;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION check_and_increment_rate_limit IS 
    'Atomically check and increment rate limit counter. Returns TRUE if allowed.';

-- ============================================================================
-- M010: Create Rate Limit bucket cleanup function
-- Should be called periodically (e.g., every 5 minutes)
-- ============================================================================
CREATE OR REPLACE FUNCTION cleanup_rate_limit_buckets() RETURNS INTEGER AS $$
DECLARE
    v_deleted INTEGER;
BEGIN
    DELETE FROM rate_limit_buckets 
    WHERE window_start < NOW() - INTERVAL '5 minutes';
    
    GET DIAGNOSTICS v_deleted = ROW_COUNT;
    RETURN v_deleted;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION cleanup_rate_limit_buckets IS 
    'Delete expired rate limit buckets. Returns number of deleted rows.';

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
DECLARE
    v_key_count INTEGER;
BEGIN
    -- Check api_keys table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'api_keys'
    ) THEN
        RAISE EXCEPTION 'api_keys table was not created';
    END IF;
    
    -- Check rate_limit_buckets table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'rate_limit_buckets'
    ) THEN
        RAISE EXCEPTION 'rate_limit_buckets table was not created';
    END IF;
    
    -- Check initial API key was seeded
    SELECT COUNT(*) INTO v_key_count FROM api_keys;
    IF v_key_count = 0 THEN
        RAISE EXCEPTION 'No API keys were seeded';
    END IF;
    
    -- Check functions exist
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc WHERE proname = 'check_and_increment_rate_limit'
    ) THEN
        RAISE EXCEPTION 'check_and_increment_rate_limit function was not created';
    END IF;
    
    IF NOT EXISTS (
        SELECT 1 FROM pg_proc WHERE proname = 'cleanup_rate_limit_buckets'
    ) THEN
        RAISE EXCEPTION 'cleanup_rate_limit_buckets function was not created';
    END IF;
    
    RAISE NOTICE 'Migration 002 completed successfully';
    RAISE NOTICE '  - api_keys table: OK';
    RAISE NOTICE '  - rate_limit_buckets table: OK';
    RAISE NOTICE '  - Initial API key seeded: OK (key_prefix: sk_dev_)';
    RAISE NOTICE '  - check_and_increment_rate_limit function: OK';
    RAISE NOTICE '  - cleanup_rate_limit_buckets function: OK';
END $$;
