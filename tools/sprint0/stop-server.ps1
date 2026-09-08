<#
.SYNOPSIS
  停止后台运行的 CloudCLI（结束监听端口 3001 的进程）。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  run-server-hidden.ps1 以隐藏窗口启动的服务没有窗口可 Ctrl+C，本脚本
  按端口找到监听进程并结束它，随后复核端口已释放。无需管理员权限
  （进程属当前用户）。
  注意：若已注册登录自启（setup-autostart.ps1），下次登录会再次自动
  拉起；想取消自启：setup-autostart.ps1 -Remove。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\stop-server.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File .\stop-server.ps1 -Port 3002
#>
[CmdletBinding()]
param(
    [int]$Port = 3001,
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

$ErrorActionPreference = 'Stop'

# ---------- 语言 / 输出辅助 ----------
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }

function Write-Ok   { param([string]$Message) Write-Host "    [OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "    [i ] $Message"  -ForegroundColor DarkGray }
function Write-Bad  { param([string]$Message) Write-Host "    [X ] $Message"  -ForegroundColor Red }

# ---------- 1. 查找监听进程 ----------
$listeners = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
if (-not $listeners) {
    Write-Info (T "端口 $Port 没有监听进程：CloudCLI 未在运行，无需停止" "Nothing is listening on port ${Port}: CloudCLI is not running, nothing to stop")
    exit 0
}

# ---------- 2. 逐个结束 ----------
$procIds = $listeners | Select-Object -ExpandProperty OwningProcess -Unique
foreach ($procId in $procIds) {
    $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
    $name = if ($proc) { $proc.ProcessName } else { T '未知' 'unknown' }
    Stop-Process -Id $procId -Force
    Write-Ok (T "已结束进程：$name (PID $procId)" "Stopped process: $name (PID $procId)")
}

# ---------- 3. 复核端口已释放 ----------
Start-Sleep -Milliseconds 500
if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) {
    Write-Bad (T "端口 $Port 仍被占用：可能有进程随即重新监听，请手动排查（netstat -ano | findstr $Port）" "Port $Port is still busy: something re-listed immediately; check manually (netstat -ano | findstr $Port)")
    exit 1
}
Write-Ok (T "CloudCLI 已停止（端口 $Port 已释放）" "CloudCLI stopped (port $Port released)")
Write-Info (T '若注册过登录自启，下次登录会自动再次启动；取消自启：setup-autostart.ps1 -Remove' 'If login autostart is registered, it will start again on next logon; disable with setup-autostart.ps1 -Remove')
