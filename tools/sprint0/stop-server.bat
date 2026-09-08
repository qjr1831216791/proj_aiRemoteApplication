@echo off
rem ============================================================
rem  Sprint 0 - stop CloudCLI server (double-click me)
rem  Thin ASCII-only launcher: logic and bilingual messages live
rem  in stop-server.ps1 (UTF-8 BOM). Keep them in the SAME folder.
rem  For a foreground-started server, just close its window or
rem  press Ctrl+C instead.
rem ============================================================

cd /d "%~dp0"
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0stop-server.ps1"
echo.
pause
