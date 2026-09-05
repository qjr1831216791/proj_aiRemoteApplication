@echo off
rem ============================================================
rem  Sprint 0 client setup - double-click launcher
rem  Calls install-client.ps1 (must be in the SAME folder).
rem
rem  Usage:
rem    1) Edit SERVER_URL below (right-click this file -> Edit)
rem       e.g.  set "SERVER_URL=http://192.168.1.100:3001"
rem    2) Double-click this file.
rem    (If SERVER_URL is empty, you can also type it when asked.)
rem ============================================================

rem -- >>> EDIT ME: server address, e.g. http://192.168.1.100:3001 <<<
set "SERVER_URL="

if "%SERVER_URL%"=="" (
    echo SERVER_URL is not set yet.
    echo Right-click this file, choose "Edit", fill in SERVER_URL, save, then run again.
    echo.
    set /p SERVER_URL=Or type the server URL now and press Enter:
)

if "%SERVER_URL%"=="" (
    echo.
    echo No URL given. Nothing to do.
    echo.
    pause
    exit /b 1
)

cd /d "%~dp0"
echo.
echo Running install-client.ps1 -Url "%SERVER_URL%" ...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-client.ps1" -Url "%SERVER_URL%"
echo.
pause
