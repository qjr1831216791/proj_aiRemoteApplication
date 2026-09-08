@echo off
rem ============================================================
rem  Sprint 0 - start CloudCLI server (double-click me)
rem  Thin ASCII-only launcher: real logic and bilingual messages
rem  live in start-server.ps1 (UTF-8 BOM). Keep them in the SAME
rem  folder. Closing the window / Ctrl+C stops the server.
rem  (.bat stays ASCII-only on purpose: Chinese text in .bat files
rem  breaks cmd parsing on GBK codepages - newline bytes get
rem  swallowed. Chinese lives in the .ps1, which is BOM-safe.)
rem ============================================================

cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-server.ps1"
echo.
pause
