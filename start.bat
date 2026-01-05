@echo off
chcp 65001 >nul
REM financeATP - One-click startup script (Windows)
REM Usage: Double-click start.bat

echo.
echo ============================================
echo   financeATP Starting...
echo ============================================
echo.

REM Check if Docker is running
docker info >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Docker Desktop is not running.
    echo Please start Docker Desktop and try again.
    echo.
    pause
    exit /b 1
)

REM Start containers
echo [1/3] Starting containers...
docker-compose up -d

if errorlevel 1 (
    echo [ERROR] Failed to start containers.
    pause
    exit /b 1
)

REM Wait for startup
echo [2/3] Waiting for services to start...
timeout /t 10 /nobreak >nul

REM Health check using PowerShell
echo [3/3] Checking connection...
:healthcheck
powershell -Command "try { $r = Invoke-WebRequest -Uri http://localhost:3000/health -UseBasicParsing -TimeoutSec 3; if ($r.StatusCode -eq 200) { exit 0 } else { exit 1 } } catch { exit 1 }" >nul 2>&1
if errorlevel 1 (
    echo   Still starting... please wait
    timeout /t 3 /nobreak >nul
    goto healthcheck
)

echo.
echo ============================================
echo   Startup Complete!
echo ============================================
echo.
echo   API: http://localhost:3000
echo   Health: http://localhost:3000/health
echo.
echo   To stop: docker-compose down
echo.

pause
