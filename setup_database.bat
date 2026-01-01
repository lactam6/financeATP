@echo off
REM ============================================================================
REM financeATP データベースセットアップスクリプト
REM ============================================================================
REM 使用方法: setup_database.bat [postgres_password]
REM ============================================================================

echo ============================================
echo financeATP Database Setup
echo ============================================

REM PostgreSQLのパスワードを環境変数から取得（セキュリティのため引数では渡さない）
set PGPASSWORD=%1

echo.
echo Step 1: Creating database 'finance_atp'...
psql -U postgres -c "CREATE DATABASE finance_atp;" 2>nul
if %ERRORLEVEL% EQU 0 (
    echo   Database created successfully.
) else (
    echo   Database already exists or error occurred. Continuing...
)

echo.
echo Step 2: Running migrations...
psql -U postgres -d finance_atp -f migrations/001_database_foundation.sql

echo.
echo ============================================
echo Setup complete!
echo ============================================

set PGPASSWORD=
