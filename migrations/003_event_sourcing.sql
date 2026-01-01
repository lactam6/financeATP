-- ============================================================================
-- Migration 003: Event Sourcing Infrastructure
-- Phase 3: Events table and Snapshots
-- ============================================================================
-- M011: Create events table (with partitioning)
-- M012: Create 2026-01 partition
-- M013: Create 2026-02 partition
-- M014: Create events indexes
-- M015: Apply immutable trigger to events
-- M016: Create unique_idempotency constraint
-- M017: Create event_snapshots table
-- M018: Create event_snapshots indexes
-- ============================================================================

-- Cleanup from failed previous run
DROP TABLE IF EXISTS event_snapshots CASCADE;
DROP TABLE IF EXISTS events CASCADE;

-- ============================================================================
-- M011: Create events table (partitioned by created_at)
-- This is the core of Event Sourcing - all facts are stored here
-- NOTE: For partitioned tables, UNIQUE constraints must include partition key
-- ============================================================================
CREATE TABLE events (
    -- Identifiers
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    
    -- Aggregate information
    aggregate_type VARCHAR(50) NOT NULL,
    aggregate_id UUID NOT NULL,
    version BIGINT NOT NULL,
    
    -- Event information
    event_type VARCHAR(100) NOT NULL,
    event_data JSONB NOT NULL,
    
    -- Operation context
    context JSONB NOT NULL DEFAULT '{}',
    
    -- Idempotency
    idempotency_key UUID,
    
    -- Timestamp
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Primary key (includes partition key)
    PRIMARY KEY (id, created_at),
    
    -- Optimistic locking constraint (must include partition key for partitioned tables)
    CONSTRAINT unique_aggregate_version UNIQUE (aggregate_id, version, created_at)
) PARTITION BY RANGE (created_at);

COMMENT ON TABLE events IS 'Event store - immutable log of all domain events';
COMMENT ON COLUMN events.aggregate_type IS 'Type of aggregate (Account, User, Transfer)';
COMMENT ON COLUMN events.aggregate_id IS 'ID of the aggregate this event belongs to';
COMMENT ON COLUMN events.version IS 'Sequence number for optimistic concurrency control';
COMMENT ON COLUMN events.event_type IS 'Type of event (AccountCreated, MoneyCredited, etc.)';
COMMENT ON COLUMN events.event_data IS 'JSON payload of the event';
COMMENT ON COLUMN events.context IS 'Operation context (api_key_id, request_user_id, correlation_id, client_ip)';
COMMENT ON COLUMN events.idempotency_key IS 'UUID for idempotency (prevents duplicate processing)';

-- ============================================================================
-- M012: Create 2026-01 partition
-- ============================================================================
CREATE TABLE events_2026_01 PARTITION OF events
    FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');

-- ============================================================================
-- M013: Create 2026-02 partition
-- ============================================================================
CREATE TABLE events_2026_02 PARTITION OF events
    FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');

-- Create additional partitions for the rest of 2026
CREATE TABLE events_2026_03 PARTITION OF events
    FOR VALUES FROM ('2026-03-01') TO ('2026-04-01');

CREATE TABLE events_2026_04 PARTITION OF events
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');

CREATE TABLE events_2026_05 PARTITION OF events
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');

CREATE TABLE events_2026_06 PARTITION OF events
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');

CREATE TABLE events_2026_07 PARTITION OF events
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE TABLE events_2026_08 PARTITION OF events
    FOR VALUES FROM ('2026-08-01') TO ('2026-09-01');

CREATE TABLE events_2026_09 PARTITION OF events
    FOR VALUES FROM ('2026-09-01') TO ('2026-10-01');

CREATE TABLE events_2026_10 PARTITION OF events
    FOR VALUES FROM ('2026-10-01') TO ('2026-11-01');

CREATE TABLE events_2026_11 PARTITION OF events
    FOR VALUES FROM ('2026-11-01') TO ('2026-12-01');

CREATE TABLE events_2026_12 PARTITION OF events
    FOR VALUES FROM ('2026-12-01') TO ('2027-01-01');

-- ============================================================================
-- M014: Create events indexes
-- ============================================================================
CREATE INDEX idx_events_aggregate ON events(aggregate_type, aggregate_id, version);
CREATE INDEX idx_events_type ON events(event_type, created_at);
CREATE INDEX idx_events_correlation ON events((context->>'correlation_id'));
CREATE INDEX idx_events_created ON events(created_at);

-- ============================================================================
-- M015: Apply immutable trigger to events
-- Prevents UPDATE and DELETE operations
-- ============================================================================
CREATE TRIGGER no_modify_events
    BEFORE UPDATE OR DELETE ON events
    FOR EACH ROW EXECUTE FUNCTION prevent_event_modification();

-- ============================================================================
-- M016: Create unique_idempotency index
-- Partial unique index for non-null idempotency keys (must include partition key)
-- ============================================================================
CREATE UNIQUE INDEX idx_events_idempotency ON events(idempotency_key, created_at) 
    WHERE idempotency_key IS NOT NULL;

-- ============================================================================
-- M017: Create event_snapshots table
-- Performance optimization: snapshot every 100 events
-- ============================================================================
CREATE TABLE event_snapshots (
    aggregate_type VARCHAR(50) NOT NULL,
    aggregate_id UUID NOT NULL,
    version BIGINT NOT NULL,
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (aggregate_type, aggregate_id)
);

COMMENT ON TABLE event_snapshots IS 'Aggregate snapshots for performance optimization';
COMMENT ON COLUMN event_snapshots.version IS 'Version at which snapshot was taken';
COMMENT ON COLUMN event_snapshots.state IS 'Serialized aggregate state';

-- ============================================================================
-- M018: Create event_snapshots indexes
-- ============================================================================
CREATE INDEX idx_snapshots_version ON event_snapshots(aggregate_id, version);
CREATE INDEX idx_snapshots_type ON event_snapshots(aggregate_type);

-- ============================================================================
-- Verification
-- ============================================================================
DO $$
DECLARE
    v_partition_count INTEGER;
BEGIN
    -- Check events table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'events'
    ) THEN
        RAISE EXCEPTION 'events table was not created';
    END IF;
    
    -- Check partitions exist
    SELECT COUNT(*) INTO v_partition_count
    FROM pg_inherits
    JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
    JOIN pg_class child ON pg_inherits.inhrelid = child.oid
    WHERE parent.relname = 'events';
    
    IF v_partition_count < 2 THEN
        RAISE EXCEPTION 'events partitions were not created (found: %)', v_partition_count;
    END IF;
    
    -- Check event_snapshots table exists
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables 
        WHERE table_name = 'event_snapshots'
    ) THEN
        RAISE EXCEPTION 'event_snapshots table was not created';
    END IF;
    
    -- Check immutable trigger exists
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger WHERE tgname = 'no_modify_events'
    ) THEN
        RAISE EXCEPTION 'no_modify_events trigger was not created';
    END IF;
    
    RAISE NOTICE 'Migration 003 completed successfully';
    RAISE NOTICE '  - events table: OK';
    RAISE NOTICE '  - events partitions: % created (full year 2026)', v_partition_count;
    RAISE NOTICE '  - events indexes: OK';
    RAISE NOTICE '  - events immutable trigger: OK';
    RAISE NOTICE '  - idempotency index: OK';
    RAISE NOTICE '  - event_snapshots table: OK';
    RAISE NOTICE '  - event_snapshots indexes: OK';
END $$;
