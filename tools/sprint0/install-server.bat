@echo off
rem ============================================================
rem  Sprint 0 server installer - double-click launcher
rem  Calls install-server.ps1 (must be in the SAME folder).
rem
rem  Usage:
rem    1) Keep this file next to install-server.ps1
rem    2) Double-click it (UAC prompt will appear - click Yes)
rem
rem  Optional: append extra args for install-server.ps1,
rem  e.g. use China npm mirror:
rem    set "PS_ARGS=-UseMirror"
rem
rem  NOTE: avoid single-quote characters (') in the folder path.
rem ============================================================

rem -- extra arguments passed to install-server.ps1
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
echo Running install-server.ps1 %PS_ARGS% ...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-server.ps1" %PS_ARGS%
set "RC=%errorlevel%"

echo.
if "%RC%"=="0" (
    echo [DONE] Server install finished. Review messages above.
) else (
    echo [FAIL] Exit code %RC%. Fix the issue above, then run again.
)
echo.
pause
