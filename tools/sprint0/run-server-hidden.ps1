<#
.SYNOPSIS
  幂等后台启动 CloudCLI：端口 3001 已在监听则直接退出，否则以隐藏窗口拉起。

.DESCRIPTION
  供两处共用：
    1. setup-autostart.ps1 注册的登录计划任务
    2. Claude Code 的 SessionStart hook（docs/research/sprint0-cloudcli-lan-deploy.md）
  特点：立即返回（不阻塞调用方）、可重复执行、进程与调用方解耦。

  日志：%TEMP%\cloudcli.log

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\run-server-hidden.ps1
#>
[CmdletBinding()]
param(
    [int]$Port = 3001
)

# 已在运行则无事可做（幂等守卫，hook/任务重复触发时为无操作）
if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { exit 0 }

# 计划任务/hook 环境下 PATH 可能未包含 npm 全局目录，从注册表刷新
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
            [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (Get-Command cloudcli -ErrorAction SilentlyContinue)) { exit 1 }

$log = Join-Path $env:TEMP 'cloudcli.log'
"==== $(Get-Date -Format s) run-server-hidden: starting cloudcli on port $Port ====" | Add-Content -Path $log

# 经 cmd 分离启动：不阻塞本脚本，输出与错误重定向到日志
Start-Process -WindowStyle Hidden -FilePath 'cmd.exe' `
    -ArgumentList "/c cloudcli 1>>`"%TEMP%\cloudcli.log`" 2>&1"
exit 0
