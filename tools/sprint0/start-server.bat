@echo off
rem ============================================================
rem  Sprint 0 - start CloudCLI server (double-click me)
rem  If already running on port 3001: just opens the browser.
rem  Otherwise starts CloudCLI in THIS window - keep it OPEN.
rem  Closing this window (or Ctrl+C) stops the server.
rem ============================================================

powershell -NoProfile -Command "if (Get-NetTCPConnection -LocalPort 3001 -State Listen -ErrorAction SilentlyContinue) { exit 0 } else { exit 1 }"
if %errorlevel%==0 (
    echo CloudCLI is already running on port 3001 - opening browser ...
    start "" http://localhost:3001
    timeout /t 3 >nul
    exit /b 0
)

echo Starting CloudCLI server on port 3001 ...
echo Keep this window OPEN. Press Ctrl+C to stop.
echo.
call cloudcli

echo.
echo CloudCLI exited unexpectedly. See messages above.
pause
