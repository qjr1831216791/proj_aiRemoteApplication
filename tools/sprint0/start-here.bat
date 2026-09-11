@echo off
rem ============================================================
rem  Sprint 0 - START HERE (double-click me)
rem  One entry for everything: start/stop services, status and
rem  URLs, install, HTTPS setup, client setup, autostart on/off.
rem  Thin ASCII-only launcher: the real bilingual menu lives at
rem  bin\menu.ps1 (keep the bin\ folder next to this file).
rem ============================================================

cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0bin\menu.ps1"
