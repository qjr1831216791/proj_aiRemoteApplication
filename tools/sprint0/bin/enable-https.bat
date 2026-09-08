@echo off
rem ============================================================
rem  Sprint 0 - HTTPS one-time setup (double-click me)
rem  Firewall TCP 443 + set network profiles Private + pin the
rem  DNS API host in the hosts file. Self-elevates via UAC.
rem  Thin ASCII-only launcher: logic/messages in enable-https.ps1
rem ============================================================

net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Requesting administrator privileges ...
    powershell -NoProfile -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

cd /d "%~dp0"
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0enable-https.ps1"
echo.
pause
