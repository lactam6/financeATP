#!/bin/bash
# M149: Daily Full Backup Script
# ===============================
#
# This script performs a daily full backup of the financeATP PostgreSQL database.
# It should be run via cron at a time of low activity (e.g., 2:00 AM).
#
# Cron example:
#   0 2 * * * /path/to/scripts/backup_daily.sh >> /var/log/finance_atp_backup.log 2>&1

set -euo pipefail

# =============================================
# Configuration
# =============================================

# Database connection settings
DB_NAME="${DB_NAME:-finance_atp}"
DB_USER="${DB_USER:-postgres}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"

# Backup directories
BACKUP_DIR="${BACKUP_DIR:-/var/backups/finance_atp}"
BASE_BACKUP_DIR="${BACKUP_DIR}/base"
WAL_ARCHIVE_DIR="${WAL_ARCHIVE_DIR:-/var/lib/postgresql/wal_archive}"

# Timestamp for this backup
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
DATE_DIR=$(date +"%Y/%m")

# Full backup path
BACKUP_PATH="${BASE_BACKUP_DIR}/${DATE_DIR}"
BACKUP_FILE="finance_atp_${TIMESTAMP}.tar.gz"

# Retention period (days)
RETENTION_DAYS=${RETENTION_DAYS:-30}

# =============================================
# Functions
# =============================================

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1"
}

error_exit() {
    log "ERROR: $1"
    exit 1
}

check_prerequisites() {
    log "Checking prerequisites..."
    
    # Check pg_basebackup is available
    if ! command -v pg_basebackup &> /dev/null; then
        error_exit "pg_basebackup not found. Please install PostgreSQL client tools."
    fi
    
    # Check pg_dump is available
    if ! command -v pg_dump &> /dev/null; then
        error_exit "pg_dump not found. Please install PostgreSQL client tools."
    fi
    
    # Check database connectivity
    if ! pg_isready -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" &> /dev/null; then
        error_exit "Cannot connect to PostgreSQL at ${DB_HOST}:${DB_PORT}"
    fi
    
    log "Prerequisites check passed"
}

create_backup_directories() {
    log "Creating backup directories..."
    mkdir -p "$BACKUP_PATH"
    mkdir -p "${BACKUP_DIR}/sql"
    log "Backup directories created: $BACKUP_PATH"
}

# =============================================
# Backup Methods
# =============================================

# Method 1: pg_basebackup (for full cluster backup with PITR support)
perform_base_backup() {
    log "Starting base backup with pg_basebackup..."
    
    local BASE_BACKUP_FILE="${BACKUP_PATH}/base_${TIMESTAMP}.tar.gz"
    
    pg_basebackup \
        -h "$DB_HOST" \
        -p "$DB_PORT" \
        -U "$DB_USER" \
        -D - \
        -Ft \
        -z \
        -P \
        -X stream \
        > "$BASE_BACKUP_FILE" 2>&1
    
    if [ $? -eq 0 ]; then
        log "Base backup completed: $BASE_BACKUP_FILE"
        log "Backup size: $(du -h "$BASE_BACKUP_FILE" | cut -f1)"
        return 0
    else
        log "Base backup failed"
        return 1
    fi
}

# Method 2: pg_dump (for logical backup)
perform_sql_backup() {
    log "Starting SQL dump backup..."
    
    local SQL_BACKUP_FILE="${BACKUP_DIR}/sql/finance_atp_${TIMESTAMP}.sql.gz"
    
    pg_dump \
        -h "$DB_HOST" \
        -p "$DB_PORT" \
        -U "$DB_USER" \
        -d "$DB_NAME" \
        --format=plain \
        --no-owner \
        --no-privileges \
        | gzip > "$SQL_BACKUP_FILE"
    
    if [ $? -eq 0 ]; then
        log "SQL backup completed: $SQL_BACKUP_FILE"
        log "Backup size: $(du -h "$SQL_BACKUP_FILE" | cut -f1)"
        return 0
    else
        log "SQL backup failed"
        return 1
    fi
}

# Method 3: Custom format backup (recommended for large databases)
perform_custom_backup() {
    log "Starting custom format backup..."
    
    local CUSTOM_BACKUP_FILE="${BACKUP_PATH}/finance_atp_${TIMESTAMP}.dump"
    
    pg_dump \
        -h "$DB_HOST" \
        -p "$DB_PORT" \
        -U "$DB_USER" \
        -d "$DB_NAME" \
        --format=custom \
        --compress=9 \
        --file="$CUSTOM_BACKUP_FILE"
    
    if [ $? -eq 0 ]; then
        log "Custom backup completed: $CUSTOM_BACKUP_FILE"
        log "Backup size: $(du -h "$CUSTOM_BACKUP_FILE" | cut -f1)"
        return 0
    else
        log "Custom backup failed"
        return 1
    fi
}

# Archive WAL files
archive_wal_files() {
    if [ -d "$WAL_ARCHIVE_DIR" ]; then
        log "Archiving WAL files..."
        
        local WAL_ARCHIVE_FILE="${BACKUP_PATH}/wal_${TIMESTAMP}.tar.gz"
        
        tar -czf "$WAL_ARCHIVE_FILE" -C "$WAL_ARCHIVE_DIR" . 2>/dev/null || true
        
        if [ -f "$WAL_ARCHIVE_FILE" ]; then
            log "WAL archive created: $WAL_ARCHIVE_FILE"
        fi
    else
        log "WAL archive directory not found, skipping"
    fi
}

# Verify backup integrity
verify_backup() {
    local BACKUP_FILE="$1"
    
    log "Verifying backup integrity..."
    
    if [ -f "$BACKUP_FILE" ]; then
        # Check if file is a valid gzip
        if gzip -t "$BACKUP_FILE" 2>/dev/null; then
            log "Backup verification passed"
            return 0
        else
            log "Backup verification failed - file may be corrupt"
            return 1
        fi
    else
        log "Backup file not found"
        return 1
    fi
}

# =============================================
# Main Execution
# =============================================

main() {
    log "=========================================="
    log "financeATP Daily Backup Started"
    log "=========================================="
    log "Database: ${DB_NAME}@${DB_HOST}:${DB_PORT}"
    log "Backup directory: ${BACKUP_PATH}"
    
    check_prerequisites
    create_backup_directories
    
    # Perform backups
    local BACKUP_SUCCESS=true
    
    # Primary: Custom format backup (best for restores)
    if ! perform_custom_backup; then
        BACKUP_SUCCESS=false
    fi
    
    # Secondary: SQL dump (for portability)
    if ! perform_sql_backup; then
        log "Warning: SQL backup failed, but custom backup may have succeeded"
    fi
    
    # Archive WAL files
    archive_wal_files
    
    if [ "$BACKUP_SUCCESS" = true ]; then
        log "=========================================="
        log "Backup completed successfully"
        log "=========================================="
        exit 0
    else
        log "=========================================="
        log "Backup completed with errors"
        log "=========================================="
        exit 1
    fi
}

# Run main function
main "$@"
