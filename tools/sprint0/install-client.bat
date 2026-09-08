@echo off
rem ============================================================
rem  Sprint 0 client setup - double-click launcher
rem  Thin ASCII-only launcher: logic and bilingual messages live
rem  in install-client.ps1 (UTF-8 BOM). Keep them in the SAME
rem  folder.
rem
rem  Usage:
rem    1) (Optional) Pre-fill SERVER_URL below, e.g.
rem       set "SERVER_URL=http://192.168.1.100:3001"
rem    2) Double-click this file. If SERVER_URL is empty the
rem       script asks interactively (bilingual), and remembers
rem       the address after the first successful connection.
rem ============================================================

rem -- >>> OPTIONAL prefill, e.g. http://192.168.1.100:3001 <<<
set "SERVER_URL="

cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-client.ps1" -Url "%SERVER_URL%"
echo.
pause
