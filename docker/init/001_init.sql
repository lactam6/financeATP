-- financeATP Database Initialization
-- This file runs all migrations in the correct order

\echo 'Running financeATP database migrations...'

\echo 'Executing: 001_database_foundation.sql'
\i /migrations/001_database_foundation.sql

\echo 'Executing: 002_auth_tables.sql'
\i /migrations/002_auth_tables.sql

\echo 'Executing: 003_event_sourcing.sql'
\i /migrations/003_event_sourcing.sql

\echo 'Executing: 004_users.sql'
\i /migrations/004_users.sql

\echo 'Executing: 005_accounts.sql'
\i /migrations/005_accounts.sql

\echo 'Executing: 006_ledger.sql'
\i /migrations/006_ledger.sql

\echo 'Executing: 007_idempotency_audit.sql'
\i /migrations/007_idempotency_audit.sql

\echo 'Executing: 099_test_database.sql'
\i /migrations/099_test_database.sql

\echo 'Database initialization complete!'
