#!/bin/bash
# financeATP Entrypoint Script
# Waits for PostgreSQL, runs migrations, then starts the app

set -e

echo "============================================"
echo "  financeATP - Initializing..."
echo "============================================"

# Wait for PostgreSQL to be ready
echo "[1/3] Waiting for PostgreSQL..."
until PGPASSWORD="${DB_PASSWORD:-password}" psql -h "${DB_HOST:-db}" -U "${DB_USER:-postgres}" -d "${DB_NAME:-finance_atp}" -c '\q' 2>/dev/null; do
    echo "  PostgreSQL is not ready yet. Waiting..."
    sleep 2
done
echo "[1/3] PostgreSQL is ready!"

# Run migrations if tables don't exist
echo "[2/3] Checking database schema..."
TABLE_COUNT=$(PGPASSWORD="${DB_PASSWORD:-password}" psql -h "${DB_HOST:-db}" -U "${DB_USER:-postgres}" -d "${DB_NAME:-finance_atp}" -tAc "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';")

if [ "$TABLE_COUNT" -lt "10" ]; then
    echo "[2/3] Running database migrations..."
    for file in /app/migrations/*.sql; do
        if [ -f "$file" ]; then
            echo "  Executing: $(basename $file)"
            PGPASSWORD="${DB_PASSWORD:-password}" psql -h "${DB_HOST:-db}" -U "${DB_USER:-postgres}" -d "${DB_NAME:-finance_atp}" -f "$file" -q
        fi
    done
    echo "[2/3] Migrations complete!"
else
    echo "[2/3] Database already initialized (found $TABLE_COUNT tables)"
fi

echo "[3/3] Starting financeATP server..."
echo "============================================"

# Start the app
exec finance_atp
