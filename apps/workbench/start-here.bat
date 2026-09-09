@echo off
rem =====================================================
rem  AI Remote Workbench - dev menu launcher
rem  Double-click me. ASCII only (repo convention:
rem  .bat stays pure ASCII; all texts live in menu.ps1).
rem =====================================================
setlocal
set "HERE=%~dp0"
if not exist "%HERE%menu.ps1" (
    echo [ERROR] menu.ps1 not found next to this file.
    pause
    exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%HERE%menu.ps1" %*
endlocal
