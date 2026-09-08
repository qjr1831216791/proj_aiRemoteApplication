@echo off
chcp 65001 >nul
rem ============================================================
rem  Sprint 0 client setup - double-click launcher ｜ 客户端配置：双击运行
rem  Calls install-client.ps1 (must be in the SAME folder).
rem  需与 install-client.ps1 在同一文件夹。
rem
rem  Usage 用法：
rem    1) (Optional) Pre-fill SERVER_URL below: right-click this file -> Edit
rem       （可选）预填 SERVER_URL：右键本文件 -> 编辑
rem       e.g. 例如：set "SERVER_URL=http://192.168.1.100:3001"
rem    2) Double-click this file. Leave SERVER_URL empty to be asked:
rem       first run types the address once; after a successful connection
rem       it is remembered, and later runs just press Enter to confirm.
rem       然后双击本文件。SERVER_URL 留空时会交互式询问：首次输入一次，
rem       连通成功后自动记住，之后每次直接回车确认即可。
rem ============================================================

rem -- >>> OPTIONAL 预填服务端地址（留空则交互询问），e.g. 例如 http://192.168.1.100:3001 <<<
set "SERVER_URL="

cd /d "%~dp0"
if not "%SERVER_URL%"=="" (
    echo Running install-client.ps1 -Url "%SERVER_URL%" ... 正在执行……
    echo.
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-client.ps1" -Url "%SERVER_URL%"
echo.
pause
