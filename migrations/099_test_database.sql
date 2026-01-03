-- ============================================================================
-- Database Integration Test
-- Tests all Phase 1-7 components
-- ============================================================================
-- Run this after all migrations are complete
-- ============================================================================

\echo '============================================'
\echo 'financeATP Database Integration Test'
\echo '============================================'

-- ============================================================================
-- Test 1: Check all tables exist
-- ============================================================================
\echo ''
\echo 'Test 1: Checking all tables exist...'

DO $$
DECLARE
    v_tables TEXT[] := ARRAY[
        'api_keys',
        'rate_limit_buckets', 
        'events',
        'event_snapshots',
        'users',
        'account_types',
        'accounts',
        'account_balances',
        'ledger_entries',
        'idempotency_keys',
        'audit_logs'
    ];
    v_table TEXT;
    v_missing TEXT[] := ARRAY[]::TEXT[];
BEGIN
    FOREACH v_table IN ARRAY v_tables LOOP
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.tables 
            WHERE table_name = v_table
        ) THEN
            v_missing := array_append(v_missing, v_table);
        END IF;
    END LOOP;
    
    IF array_length(v_missing, 1) > 0 THEN
        RAISE EXCEPTION 'Missing tables: %', array_to_string(v_missing, ', ');
    END IF;
    
    RAISE NOTICE 'OK: All % tables exist', array_length(v_tables, 1);
END $$;

-- ============================================================================
-- Test 2: Check all functions exist
-- ============================================================================
\echo ''
\echo 'Test 2: Checking all functions exist...'

DO $$
DECLARE
    v_functions TEXT[] := ARRAY[
        'prevent_event_modification',
        'check_and_increment_rate_limit',
        'cleanup_rate_limit_buckets',
        'get_wallet_account_id',
        'check_user_wallet_only',
        'check_ledger_balance_batch',
        'reset_stale_idempotency_keys',
        'calculate_audit_hash',
        'verify_audit_chain'
    ];
    v_func TEXT;
    v_missing TEXT[] := ARRAY[]::TEXT[];
BEGIN
    FOREACH v_func IN ARRAY v_functions LOOP
        IF NOT EXISTS (
            SELECT 1 FROM pg_proc WHERE proname = v_func
        ) THEN
            v_missing := array_append(v_missing, v_func);
        END IF;
    END LOOP;
    
    IF array_length(v_missing, 1) > 0 THEN
        RAISE EXCEPTION 'Missing functions: %', array_to_string(v_missing, ', ');
    END IF;
    
    RAISE NOTICE 'OK: All % functions exist', array_length(v_functions, 1);
END $$;

-- ============================================================================
-- Test 3: Check system users exist
-- ============================================================================
\echo ''
\echo 'Test 3: Checking system users...'

DO $$
DECLARE
    v_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO v_count FROM users WHERE is_system = TRUE;
    
    IF v_count != 3 THEN
        RAISE EXCEPTION 'Expected 3 system users, found %', v_count;
    END IF;
    
    -- Check specific users
    IF NOT EXISTS (SELECT 1 FROM users WHERE username = 'SYSTEM_MINT') THEN
        RAISE EXCEPTION 'SYSTEM_MINT user not found';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM users WHERE username = 'SYSTEM_FEE') THEN
        RAISE EXCEPTION 'SYSTEM_FEE user not found';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM users WHERE username = 'SYSTEM_RESERVE') THEN
        RAISE EXCEPTION 'SYSTEM_RESERVE user not found';
    END IF;
    
    RAISE NOTICE 'OK: All 3 system users exist (SYSTEM_MINT, SYSTEM_FEE, SYSTEM_RESERVE)';
END $$;

-- ============================================================================
-- Test 4: Check system accounts and balances exist
-- ============================================================================
\echo ''
\echo 'Test 4: Checking system accounts and balances...'

DO $$
DECLARE
    v_account_count INTEGER;
    v_balance_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO v_account_count 
    FROM accounts a JOIN users u ON a.user_id = u.id WHERE u.is_system = TRUE;
    
    SELECT COUNT(*) INTO v_balance_count FROM account_balances;
    
    IF v_account_count != 3 THEN
        RAISE EXCEPTION 'Expected 3 system accounts, found %', v_account_count;
    END IF;
    
    IF v_balance_count != 3 THEN
        RAISE EXCEPTION 'Expected 3 balance records, found %', v_balance_count;
    END IF;
    
    RAISE NOTICE 'OK: 3 system accounts and 3 balance records exist';
END $$;

-- ============================================================================
-- Test 5: Test user creation with automatic wallet
-- ============================================================================
\echo ''
\echo 'Test 5: Testing user creation with wallet...'

DO $$
DECLARE
    v_user_id UUID := gen_random_uuid();
    v_account_id UUID;
BEGIN
    -- Create test user
    INSERT INTO users (id, username, email, display_name, created_at, updated_at)
    VALUES (v_user_id, 'test_user_001', 'test001@example.com', 'Test User', NOW(), NOW());
    
    -- Create wallet account
    INSERT INTO accounts (user_id, account_type)
    VALUES (v_user_id, 'user_wallet')
    RETURNING id INTO v_account_id;
    
    -- Create balance record
    INSERT INTO account_balances (account_id, balance, last_event_id, last_event_version)
    VALUES (v_account_id, 0, '00000000-0000-0000-0000-000000000000', 0);
    
    -- Test get_wallet_account_id function
    IF get_wallet_account_id(v_user_id) != v_account_id THEN
        RAISE EXCEPTION 'get_wallet_account_id returned wrong account';
    END IF;
    
    -- Clean up
    DELETE FROM account_balances WHERE account_id = v_account_id;
    DELETE FROM accounts WHERE id = v_account_id;
    DELETE FROM users WHERE id = v_user_id;
    
    RAISE NOTICE 'OK: User creation with wallet works correctly';
END $$;

-- ============================================================================
-- Test 6: Test user_wallet_only constraint
-- ============================================================================
\echo ''
\echo 'Test 6: Testing user_wallet_only constraint...'

DO $$
DECLARE
    v_user_id UUID := gen_random_uuid();
BEGIN
    -- Create regular (non-system) user
    INSERT INTO users (id, username, email, display_name, created_at, updated_at)
    VALUES (v_user_id, 'test_user_002', 'test002@example.com', 'Test User 2', NOW(), NOW());
    
    -- Try to create mint_source account (should fail)
    BEGIN
        INSERT INTO accounts (user_id, account_type) VALUES (v_user_id, 'mint_source');
        RAISE EXCEPTION 'Should have failed: non-system user with mint_source account';
    EXCEPTION WHEN check_violation THEN
        -- Expected
        NULL;
    END;
    
    -- Clean up
    DELETE FROM users WHERE id = v_user_id;
    
    RAISE NOTICE 'OK: user_wallet_only constraint works correctly';
END $$;

-- ============================================================================
-- Test 7: Test events immutability
-- ============================================================================
\echo ''
\echo 'Test 7: Testing events immutability...'

DO $$
DECLARE
    v_event_id UUID;
BEGIN
    -- Insert test event
    INSERT INTO events (aggregate_type, aggregate_id, version, event_type, event_data)
    VALUES ('Test', gen_random_uuid(), 1, 'TestEvent', '{}')
    RETURNING id INTO v_event_id;
    
    -- Try to update (should fail)
    BEGIN
        UPDATE events SET event_type = 'Modified' WHERE id = v_event_id;
        RAISE EXCEPTION 'Should have failed: UPDATE on events table';
    EXCEPTION WHEN restrict_violation THEN
        -- Expected
        NULL;
    END;
    
    -- Try to delete (should fail)
    BEGIN
        DELETE FROM events WHERE id = v_event_id;
        RAISE EXCEPTION 'Should have failed: DELETE on events table';
    EXCEPTION WHEN restrict_violation THEN
        -- Expected
        NULL;
    END;
    
    RAISE NOTICE 'OK: Events table is immutable';
END $$;

-- ============================================================================
-- Test 8: Test audit log hash chain
-- ============================================================================
\echo ''
\echo 'Test 8: Testing audit log hash chain...'

DO $$
DECLARE
    v_hash1 VARCHAR(64);
    v_hash2 VARCHAR(64);
    v_prev_hash VARCHAR(64);
BEGIN
    -- Insert first audit log
    INSERT INTO audit_logs (action, resource_type)
    VALUES ('test.action1', 'Test')
    RETURNING current_hash INTO v_hash1;
    
    -- Insert second audit log
    INSERT INTO audit_logs (action, resource_type)
    VALUES ('test.action2', 'Test')
    RETURNING current_hash, previous_hash INTO v_hash2, v_prev_hash;
    
    -- Check hash chain
    IF v_prev_hash != v_hash1 THEN
        RAISE EXCEPTION 'Hash chain broken: previous_hash (%) != first hash (%)', v_prev_hash, v_hash1;
    END IF;
    
    -- Verify chain integrity
    IF EXISTS (SELECT 1 FROM verify_audit_chain() WHERE is_valid = FALSE) THEN
        RAISE EXCEPTION 'verify_audit_chain found invalid entries';
    END IF;
    
    RAISE NOTICE 'OK: Audit log hash chain works correctly';
END $$;

-- ============================================================================
-- Test 9: Test audit log immutability
-- ============================================================================
\echo ''
\echo 'Test 9: Testing audit log immutability...'

DO $$
DECLARE
    v_log_id UUID;
BEGIN
    SELECT id INTO v_log_id FROM audit_logs LIMIT 1;
    
    -- Try to update (should fail)
    BEGIN
        UPDATE audit_logs SET action = 'modified' WHERE id = v_log_id;
        RAISE EXCEPTION 'Should have failed: UPDATE on audit_logs table';
    EXCEPTION WHEN restrict_violation THEN
        -- Expected
        NULL;
    END;
    
    -- Try to delete (should fail)
    BEGIN
        DELETE FROM audit_logs WHERE id = v_log_id;
        RAISE EXCEPTION 'Should have failed: DELETE on audit_logs table';
    EXCEPTION WHEN restrict_violation THEN
        -- Expected
        NULL;
    END;
    
    RAISE NOTICE 'OK: Audit logs table is immutable';
END $$;

-- ============================================================================
-- Test 10: Test rate limiting
-- ============================================================================
\echo ''
\echo 'Test 10: Testing rate limiting...'

DO $$
DECLARE
    v_api_key_id UUID;
    v_allowed BOOLEAN;
BEGIN
    SELECT id INTO v_api_key_id FROM api_keys LIMIT 1;
    
    -- First request should be allowed
    v_allowed := check_and_increment_rate_limit(v_api_key_id, 5);
    IF NOT v_allowed THEN
        RAISE EXCEPTION 'First request should be allowed';
    END IF;
    
    -- Requests 2-5 should be allowed
    FOR i IN 2..5 LOOP
        v_allowed := check_and_increment_rate_limit(v_api_key_id, 5);
        IF NOT v_allowed THEN
            RAISE EXCEPTION 'Request % should be allowed', i;
        END IF;
    END LOOP;
    
    -- Request 6 should be denied
    v_allowed := check_and_increment_rate_limit(v_api_key_id, 5);
    IF v_allowed THEN
        RAISE EXCEPTION 'Request 6 should be denied (rate limit exceeded)';
    END IF;
    
    -- Clean up
    DELETE FROM rate_limit_buckets WHERE api_key_id = v_api_key_id;
    
    RAISE NOTICE 'OK: Rate limiting works correctly';
END $$;

-- ============================================================================
-- Test 11: Test idempotency key timeout reset
-- ============================================================================
\echo ''
\echo 'Test 11: Testing idempotency key timeout reset...'

DO $$
DECLARE
    v_key UUID := gen_random_uuid();
    v_reset_count INTEGER;
BEGIN
    -- Insert stuck processing key (started 10 minutes ago)
    INSERT INTO idempotency_keys (key, request_hash, processing_status, processing_started_at)
    VALUES (v_key, 'test_hash', 'processing', NOW() - INTERVAL '10 minutes');
    
    -- Reset stale keys
    v_reset_count := reset_stale_idempotency_keys();
    
    IF v_reset_count < 1 THEN
        RAISE EXCEPTION 'Expected at least 1 key to be reset, got %', v_reset_count;
    END IF;
    
    -- Check status changed to failed
    IF NOT EXISTS (
        SELECT 1 FROM idempotency_keys 
        WHERE key = v_key AND processing_status = 'failed'
    ) THEN
        RAISE EXCEPTION 'Key should be marked as failed';
    END IF;
    
    -- Clean up
    DELETE FROM idempotency_keys WHERE key = v_key;
    
    RAISE NOTICE 'OK: Idempotency key timeout reset works correctly';
END $$;

-- ============================================================================
-- Test 12: Test partitions exist
-- ============================================================================
\echo ''
\echo 'Test 12: Checking partitions...'

DO $$
DECLARE
    v_events_partitions INTEGER;
    v_ledger_partitions INTEGER;
BEGIN
    SELECT COUNT(*) INTO v_events_partitions
    FROM pg_inherits
    JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
    WHERE parent.relname = 'events';
    
    SELECT COUNT(*) INTO v_ledger_partitions
    FROM pg_inherits
    JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
    WHERE parent.relname = 'ledger_entries';
    
    IF v_events_partitions < 6 THEN
        RAISE EXCEPTION 'Expected at least 6 events partitions, found %', v_events_partitions;
    END IF;
    
    IF v_ledger_partitions < 6 THEN
        RAISE EXCEPTION 'Expected at least 6 ledger_entries partitions, found %', v_ledger_partitions;
    END IF;
    
    RAISE NOTICE 'OK: events has % partitions, ledger_entries has % partitions', 
        v_events_partitions, v_ledger_partitions;
END $$;

-- ============================================================================
-- Summary
-- ============================================================================
\echo ''
\echo '============================================'
\echo 'All 12 tests passed!'
\echo 'Database layer is ready for Phase 8 (Rust)'
\echo '============================================'

-- Show table summary
\echo ''
\echo 'Table Summary:'
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname || '.' || tablename)) as size
FROM pg_tables 
WHERE schemaname = 'public' 
ORDER BY tablename;
