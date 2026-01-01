#!/bin/bash
# M150: Backup Retention Policy Script
# =====================================
#
# This script enforces the backup retention policy by removing old backups.
# It should be run daily after the backup script completes.
#
# Cron example:
#   30 3 * * * /path/to/scripts/backup_retention.sh >> /var/log/finance_atp_backup.log 2>&1

set -euo pipefail

# =============================================
# Configuration
# =============================================

# Backup directory
BACKUP_DIR="${BACKUP_DIR:-/var/backups/finance_atp}"
WAL_ARCHIVE_DIR="${WAL_ARCHIVE_DIR:-/var/lib/postgresql/wal_archive}"

# Retention periods (in days)
DAILY_RETENTION=${DAILY_RETENTION:-7}      # Keep daily backups for 7 days
WEEKLY_RETENTION=${WEEKLY_RETENTION:-30}   # Keep weekly backups for 30 days
MONTHLY_RETENTION=${MONTHLY_RETENTION:-365} # Keep monthly backups for 1 year

# Minimum backups to keep (safety net)
MIN_BACKUPS_TO_KEEP=${MIN_BACKUPS_TO_KEEP:-3}

# WAL retention (in days) - should match oldest backup you might need
WAL_RETENTION=${WAL_RETENTION:-7}

# Dry run mode (set to "true" to only show what would be deleted)
DRY_RUN=${DRY_RUN:-false}

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

# Get the day of the week for a file (0=Sunday, 1=Monday, etc.)
get_day_of_week() {
    local FILE="$1"
    date -r "$FILE" +%w 2>/dev/null || echo "-1"
}

# Get the day of the month for a file
get_day_of_month() {
    local FILE="$1"
    date -r "$FILE" +%d 2>/dev/null || echo "-1"
}

# Check if a file is a weekly backup (created on Sunday)
is_weekly_backup() {
    local FILE="$1"
    [ "$(get_day_of_week "$FILE")" -eq 0 ]
}

# Check if a file is a monthly backup (created on the 1st)
is_monthly_backup() {
    local FILE="$1"
    [ "$(get_day_of_month "$FILE")" -eq "01" ]
}

# Delete a file (or simulate deletion in dry run mode)
delete_file() {
    local FILE="$1"
    local REASON="$2"
    
    if [ "$DRY_RUN" = "true" ]; then
        log "[DRY RUN] Would delete: $FILE ($REASON)"
    else
        rm -f "$FILE"
        log "Deleted: $FILE ($REASON)"
    fi
}

# =============================================
# Cleanup Functions
# =============================================

cleanup_daily_backups() {
    log "Cleaning up daily backups (retention: ${DAILY_RETENTION} days)..."
    
    local COUNT=0
    local DELETED=0
    
    # Find backup files older than daily retention
    while IFS= read -r -d '' FILE; do
        COUNT=$((COUNT + 1))
        
        # Skip if it's a weekly backup
        if is_weekly_backup "$FILE"; then
            log "Keeping weekly backup: $FILE"
            continue
        fi
        
        # Skip if it's a monthly backup
        if is_monthly_backup "$FILE"; then
            log "Keeping monthly backup: $FILE"
            continue
        fi
        
        # Delete daily backup older than retention period
        delete_file "$FILE" "daily backup older than ${DAILY_RETENTION} days"
        DELETED=$((DELETED + 1))
        
    done < <(find "${BACKUP_DIR}" -name "*.dump" -type f -mtime +${DAILY_RETENTION} -print0 2>/dev/null)
    
    log "Daily cleanup: found $COUNT files, deleted $DELETED"
}

cleanup_weekly_backups() {
    log "Cleaning up weekly backups (retention: ${WEEKLY_RETENTION} days)..."
    
    local DELETED=0
    
    # Find Sunday backups older than weekly retention but younger than monthly
    while IFS= read -r -d '' FILE; do
        if is_weekly_backup "$FILE" && ! is_monthly_backup "$FILE"; then
            # Get file age in days
            local AGE_DAYS=$(( ($(date +%s) - $(date -r "$FILE" +%s)) / 86400 ))
            
            if [ "$AGE_DAYS" -gt "$WEEKLY_RETENTION" ]; then
                delete_file "$FILE" "weekly backup older than ${WEEKLY_RETENTION} days"
                DELETED=$((DELETED + 1))
            fi
        fi
    done < <(find "${BACKUP_DIR}" -name "*.dump" -type f -mtime +${DAILY_RETENTION} -print0 2>/dev/null)
    
    log "Weekly cleanup: deleted $DELETED files"
}

cleanup_monthly_backups() {
    log "Cleaning up monthly backups (retention: ${MONTHLY_RETENTION} days)..."
    
    local DELETED=0
    
    # Find monthly backups older than monthly retention
    while IFS= read -r -d '' FILE; do
        if is_monthly_backup "$FILE"; then
            local AGE_DAYS=$(( ($(date +%s) - $(date -r "$FILE" +%s)) / 86400 ))
            
            if [ "$AGE_DAYS" -gt "$MONTHLY_RETENTION" ]; then
                delete_file "$FILE" "monthly backup older than ${MONTHLY_RETENTION} days"
                DELETED=$((DELETED + 1))
            fi
        fi
    done < <(find "${BACKUP_DIR}" -name "*.dump" -type f -print0 2>/dev/null)
    
    log "Monthly cleanup: deleted $DELETED files"
}

cleanup_sql_backups() {
    log "Cleaning up SQL backups (retention: ${DAILY_RETENTION} days)..."
    
    local DELETED=0
    
    while IFS= read -r -d '' FILE; do
        delete_file "$FILE" "SQL backup older than ${DAILY_RETENTION} days"
        DELETED=$((DELETED + 1))
    done < <(find "${BACKUP_DIR}/sql" -name "*.sql.gz" -type f -mtime +${DAILY_RETENTION} -print0 2>/dev/null)
    
    log "SQL cleanup: deleted $DELETED files"
}

cleanup_wal_archives() {
    log "Cleaning up WAL archives (retention: ${WAL_RETENTION} days)..."
    
    if [ ! -d "$WAL_ARCHIVE_DIR" ]; then
        log "WAL archive directory not found, skipping"
        return
    fi
    
    local DELETED=0
    
    while IFS= read -r -d '' FILE; do
        delete_file "$FILE" "WAL file older than ${WAL_RETENTION} days"
        DELETED=$((DELETED + 1))
    done < <(find "${WAL_ARCHIVE_DIR}" -type f -mtime +${WAL_RETENTION} -print0 2>/dev/null)
    
    log "WAL cleanup: deleted $DELETED files"
}

cleanup_empty_directories() {
    log "Cleaning up empty directories..."
    
    find "${BACKUP_DIR}" -type d -empty -delete 2>/dev/null || true
    
    log "Empty directory cleanup completed"
}

verify_minimum_backups() {
    log "Verifying minimum backup count..."
    
    local BACKUP_COUNT=$(find "${BACKUP_DIR}" -name "*.dump" -type f 2>/dev/null | wc -l)
    
    if [ "$BACKUP_COUNT" -lt "$MIN_BACKUPS_TO_KEEP" ]; then
        log "WARNING: Only $BACKUP_COUNT backups found (minimum: $MIN_BACKUPS_TO_KEEP)"
        log "Consider checking backup job status"
    else
        log "Backup count verified: $BACKUP_COUNT backups available"
    fi
}

show_backup_summary() {
    log "=========================================="
    log "Backup Summary"
    log "=========================================="
    
    if [ -d "$BACKUP_DIR" ]; then
        log "Backup directory size: $(du -sh "$BACKUP_DIR" 2>/dev/null | cut -f1)"
        log "Total custom backups: $(find "$BACKUP_DIR" -name "*.dump" -type f 2>/dev/null | wc -l)"
        log "Total SQL backups: $(find "$BACKUP_DIR/sql" -name "*.sql.gz" -type f 2>/dev/null | wc -l)"
    fi
    
    if [ -d "$WAL_ARCHIVE_DIR" ]; then
        log "WAL archive size: $(du -sh "$WAL_ARCHIVE_DIR" 2>/dev/null | cut -f1)"
        log "WAL files: $(find "$WAL_ARCHIVE_DIR" -type f 2>/dev/null | wc -l)"
    fi
    
    log ""
    log "Oldest backup:"
    find "$BACKUP_DIR" -name "*.dump" -type f -printf '%T+ %p\n' 2>/dev/null | sort | head -1 || echo "None"
    
    log "Newest backup:"
    find "$BACKUP_DIR" -name "*.dump" -type f -printf '%T+ %p\n' 2>/dev/null | sort -r | head -1 || echo "None"
}

# =============================================
# Main Execution
# =============================================

main() {
    log "=========================================="
    log "financeATP Backup Retention Policy"
    log "=========================================="
    log "Backup directory: ${BACKUP_DIR}"
    log "Daily retention: ${DAILY_RETENTION} days"
    log "Weekly retention: ${WEEKLY_RETENTION} days"
    log "Monthly retention: ${MONTHLY_RETENTION} days"
    log "WAL retention: ${WAL_RETENTION} days"
    
    if [ "$DRY_RUN" = "true" ]; then
        log ""
        log "*** DRY RUN MODE - No files will be deleted ***"
        log ""
    fi
    
    # Check backup directory exists
    if [ ! -d "$BACKUP_DIR" ]; then
        log "Backup directory does not exist: $BACKUP_DIR"
        log "Nothing to clean up"
        exit 0
    fi
    
    # Run cleanup tasks
    cleanup_daily_backups
    cleanup_weekly_backups
    cleanup_monthly_backups
    cleanup_sql_backups
    cleanup_wal_archives
    cleanup_empty_directories
    
    # Verification and summary
    verify_minimum_backups
    show_backup_summary
    
    log "=========================================="
    log "Retention policy enforcement completed"
    log "=========================================="
}

# Run main function
main "$@"
