@echo off
rem ============================================================
rem  Sprint 0 - DISABLE autostart (double-click me)
rem  Removes logon autostart for CloudCLI + Caddy + ddns-go.
rem  Running processes are NOT killed; they just will not come
rem  back after the next reboot/logon.
rem  To ENABLE again, double-click autostart-on.bat
rem ============================================================

cd /d "%~dp0"
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup-autostart.ps1" -Remove
echo.
pause
