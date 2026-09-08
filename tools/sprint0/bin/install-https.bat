@echo off
rem ============================================================
rem  Sprint 0 HTTPS stack installer - double-click launcher
rem  Downloads caddy.exe + ddns-go.exe into the stack dir,
rem  generates Caddyfile, runs enable-https.ps1, starts ddns-go.
rem  Thin ASCII-only launcher: logic/messages in install-https.ps1
rem  (must be in the SAME folder).
rem
rem  Usage:
rem    1) Keep this file next to install-https.ps1
rem    2) Double-click it (UAC prompt will appear - click Yes)
rem
rem  Optional: append extra args for install-https.ps1, e.g.:
rem    set "PS_ARGS=-Update"
rem    set "PS_ARGS=-CaddyZip C:\Users\me\Downloads\caddy_2.11.4_windows_amd64.zip"
rem    (use -DdnsZip the same way for ddns-go)
rem
rem  NOTE: avoid single-quote characters (') in the folder path.
rem ============================================================

rem -- extra arguments passed to install-https.ps1
set "PS_ARGS="

rem -- relaunch self as administrator if not elevated
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Requesting administrator privileges ...
    powershell -NoProfile -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

cd /d "%~dp0"
echo.
echo Running install-https.ps1 %PS_ARGS% ...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-https.ps1" %PS_ARGS%
set "RC=%errorlevel%"

echo.
if "%RC%"=="0" (
    echo [DONE] HTTPS stack install finished. Review messages above.
) else (
    echo [FAIL] Exit code %RC%. Fix the issue above, then run again.
)
echo.
pause
