-- ============================================================================
-- Migration 004: Users Table
-- Phase 4: User related tables
-- ============================================================================
-- M019: Create users table
-- M020: Create users input validation constraints
-- M021: Create users indexes
-- M022: Seed system users (SYSTEM_MINT, SYSTEM_FEE, SYSTEM_RESERVE)
-- ============================================================================

-- ============================================================================
-- M019: Create users table
-- User information (authentication is handled by Next.js)
-- ============================================================================
CREATE TABLE users (
    id UUID PRIMARY KEY,
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(100) UNIQUE NOT NULL,
    display_name VARCHAR(100),
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    deleted_at TIMESTAMPTZ
);

COMMENT ON TABLE users IS 'User information (authentication handled by Next.js)';
COMMENT ON COLUMN users.is_system IS 'TRUE for system users (SYSTEM_MINT, etc.)';
COMMENT ON COLUMN users.deleted_at IS 'Soft delete timestamp';

-- ============================================================================
-- M020: Create users input validation constraints
-- ============================================================================
ALTER TABLE users ADD CONSTRAINT valid_username CHECK (
    LENGTH(username) >= 3 AND 
    username ~ '^[a-zA-Z0-9_]+$'
);

ALTER TABLE users ADD CONSTRAINT valid_email CHECK (
    email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'
);

-- ============================================================================
-- M021: Create users indexes
-- ============================================================================
CREATE INDEX idx_users_active ON users(id) 
    WHERE deleted_at IS NULL AND is_system = FALSE;

CREATE INDEX idx_users_email ON users(email) 
    WHERE deleted_at IS NULL;

CREATE INDEX idx_users_username ON users(username) 
    WHERE deleted_at IS NULL;

CREATE INDEX idx_users_system ON users(id) 
    WHERE is_system = TRUE;

-- ============================================================================
-- M022: Seed system users
-- These are required for double-entry bookkeeping
-- ============================================================================
INSERT INTO users (id, username, email, display_name, is_system, created_at, updated_at) VALUES
    (
        '00000000-0000-0000-0000-000000000001',
        'SYSTEM_MINT',
        'mint@system.internal',
        'ATP Mint Source',
        TRUE,
        NOW(),
        NOW()
    ),
    (
        '00000000-0000-0000-0000-000000000002',
        'SYSTEM_BURN',
        'burn@system.internal',
        'ATP Burn Sink',
        TRUE,
        NOW(),
        NOW()
    ),
    (
        '00000000-0000-0000-0000-000000000003',
        'SYSTEM_FEE',
        'fee@system.internal',
        'Fee Income',
        TRUE,
        NOW(),
        NOW()
    ),
    (
        '00000000-0000-0000-0000-000000000004',
        'SYSTEM_RESERVE',
        'reserve@system.internal',
        'System Reserve',
        TRUE,
        NOW(),
        NOW()
    );

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
DECLARE
    v_system_count INTEGER;
BEGIN
    -- Check users table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'users'
    ) THEN
        RAISE EXCEPTION 'users table was not created';
    END IF;
    
    -- Check constraints exist
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'valid_username'
    ) THEN
        RAISE EXCEPTION 'valid_username constraint was not created';
    END IF;
    
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'valid_email'
    ) THEN
        RAISE EXCEPTION 'valid_email constraint was not created';
    END IF;
    
    -- Check system users exist
    SELECT COUNT(*) INTO v_system_count FROM users WHERE is_system = TRUE;
    IF v_system_count != 3 THEN
        RAISE EXCEPTION 'Expected 3 system users, found %', v_system_count;
    END IF;
    
    RAISE NOTICE 'Migration 004 completed successfully';
    RAISE NOTICE '  - users table: OK';
    RAISE NOTICE '  - valid_username constraint: OK';
    RAISE NOTICE '  - valid_email constraint: OK';
    RAISE NOTICE '  - indexes: OK';
    RAISE NOTICE '  - system users: % seeded (SYSTEM_MINT, SYSTEM_FEE, SYSTEM_RESERVE)', v_system_count;
END $$;
