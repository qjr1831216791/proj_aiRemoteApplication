@echo off
rem ============================================================
rem  Sprint 0 - ENABLE autostart (double-click me)
rem  Enables logon autostart for CloudCLI + Caddy + ddns-go.
rem  Thin ASCII-only launcher; logic and bilingual messages live
rem  in setup-autostart.ps1 (UTF-8 BOM). Same folder required.
rem  To DISABLE instead, double-click autostart-off.bat
rem ============================================================

cd /d "%~dp0"
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup-autostart.ps1"
echo.
pause
