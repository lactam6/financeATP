@echo off
REM financeATP Database Setup Script
REM Usage: setup_db.bat

set PGPASSWORD=qwerty0829

echo [1/8] Running 001_database_foundation.sql...
psql -U postgres -d finance_atp -f migrations\001_database_foundation.sql

echo [2/8] Running 002_auth_tables.sql...
psql -U postgres -d finance_atp -f migrations\002_auth_tables.sql

echo [3/8] Running 003_event_sourcing.sql...
psql -U postgres -d finance_atp -f migrations\003_event_sourcing.sql

echo [4/8] Running 004_users.sql...
psql -U postgres -d finance_atp -f migrations\004_users.sql

echo [5/8] Running 005_accounts.sql...
psql -U postgres -d finance_atp -f migrations\005_accounts.sql

echo [6/8] Running 006_ledger.sql...
psql -U postgres -d finance_atp -f migrations\006_ledger.sql

echo [7/8] Running 007_idempotency_audit.sql...
psql -U postgres -d finance_atp -f migrations\007_idempotency_audit.sql

echo [8/8] Running test_database.sql...
psql -U postgres -d finance_atp -f migrations\test_database.sql

echo.
echo Database setup complete!
pause
