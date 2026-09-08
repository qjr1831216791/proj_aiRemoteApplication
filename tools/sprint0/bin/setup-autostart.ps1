<#
.SYNOPSIS
  一键注册（或移除）CloudCLI HTTPS 栈的开机自启（登录触发计划任务）。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  管理三个组件的登录自启，全部幂等（已在运行则自动跳过）：
    1. CloudCLI        -> run-server-hidden.ps1（隐藏窗口，端口守卫）
    2. Caddy           -> caddy.exe start --config Caddyfile（HTTPS 反代 443 -> 3001）
    3. ddns-go         -> ddns-go.exe（DDNS，ai.jackqi.cn 跟随本机 IP）

  用法（开关）：
    启用：powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1
    关闭：powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -Remove

  变更范围：仅操作名为 "* Sprint0 autostart" 的三个计划任务，不改其他系统配置。
  关键参数：-ExecutionTimeLimit Zero（取消默认 72h 强杀，服务需常驻）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1            # 全部启用
  powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -Remove    # 全部移除
  powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -StackDir D:\Software\cloudcli-https
#>
[CmdletBinding()]
param(
    [switch]$Remove,
    [string]$StackDir = 'D:\Software\cloudcli-https',
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

# ---------- 语言 / 输出辅助 ----------
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }
function Write-Ok   { param([string]$Message) Write-Host "[OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "[i ] $Message"  -ForegroundColor DarkGray }
function Write-Warn { param([string]$Message) Write-Host "[!] $Message"   -ForegroundColor Yellow }
function Write-Bad  { param([string]$Message) Write-Host "[X ] $Message"  -ForegroundColor Red }

# ---------- 组件定义 ----------
$runner = Join-Path $PSScriptRoot 'run-server-hidden.ps1'
$components = @(
    @{
        Name   = 'CloudCLI Sprint0 autostart'
        Exe    = 'powershell.exe'
        Arg    = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$runner`""
        Check  = $runner
        Why    = (T '缺少 run-server-hidden.ps1（需与本脚本同目录）' 'run-server-hidden.ps1 missing (must sit next to this script)')
    },
    @{
        Name   = 'Caddy Sprint0 autostart'
        Exe    = 'powershell.exe'
        Arg    = "-NoProfile -WindowStyle Hidden -Command `"& '$StackDir\caddy.exe' start --config '$StackDir\Caddyfile'`""
        Check  = (Join-Path $StackDir 'caddy.exe')
        Why    = (T "缺少 $StackDir\caddy.exe" "Missing $StackDir\caddy.exe")
    },
    @{
        Name   = 'ddns-go Sprint0 autostart'
        Exe    = 'powershell.exe'
        Arg    = "-NoProfile -WindowStyle Hidden -Command `"& '$StackDir\ddns-go.exe' -c '$StackDir\ddns-go.yaml' -l :9876 -f 300`""
        Check  = (Join-Path $StackDir 'ddns-go.exe')
        Why    = (T "缺少 $StackDir\ddns-go.exe" "Missing $StackDir\ddns-go.exe")
    }
)

# ---------- 公共任务设置 ----------
# Zero = 不限执行时长（默认 72h 会把常驻服务杀掉）；电池供电时也允许运行
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit ([TimeSpan]::Zero)
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME

# ---------- 移除模式 ----------
if ($Remove) {
    foreach ($c in $components) {
        if (Get-ScheduledTask -TaskName $c.Name -ErrorAction SilentlyContinue) {
            Unregister-ScheduledTask -TaskName $c.Name -Confirm:$false
            Write-Ok (T "已移除自启：$($c.Name)" "Autostart removed: $($c.Name)")
        } else {
            Write-Info (T "不存在，跳过：$($c.Name)" "Not present, skipped: $($c.Name)")
        }
    }
    Write-Info (T '已全部关闭。正在运行的进程不受影响（重启后不再自动拉起）。' 'All disabled. Running processes are unaffected (they simply will not restart after reboot).')
    exit 0
}

# ---------- 注册模式 ----------
foreach ($c in $components) {
    if (-not (Test-Path $c.Check)) {
        Write-Warn (T "跳过 $($c.Name)：$($c.Why)" "Skipping $($c.Name): $($c.Why)")
        continue
    }
    $action = New-ScheduledTaskAction -Execute $c.Exe -Argument $c.Arg
    try {
        if (Get-ScheduledTask -TaskName $c.Name -ErrorAction SilentlyContinue) {
            Unregister-ScheduledTask -TaskName $c.Name -Confirm:$false
        }
        Register-ScheduledTask -TaskName $c.Name -Action $action -Trigger $trigger -Settings $settings | Out-Null
        Write-Ok (T "已注册登录自启：$($c.Name)" "Login autostart registered: $($c.Name)")
    } catch {
        Write-Bad (T "注册失败 $($c.Name)：$($_.Exception.Message)" "Registration failed $($c.Name): $($_.Exception.Message)")
    }
}

Write-Host ''
Write-Info (T '生效时机：下次登录自动启动（当前已在运行的进程不受影响）。' 'Takes effect on next logon (currently running processes are untouched).')
Write-Info (T '立即启动单个组件：Start-ScheduledTask -TaskName "<任务名>"' 'Start one now: Start-ScheduledTask -TaskName "<task name>"')
Write-Info (T '关闭自启：powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -Remove' 'Disable all: powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -Remove')
Write-Info (T '查看日志：%TEMP%\cloudcli.log；ddns-go 界面：http://127.0.0.1:9876' 'Log: %TEMP%\cloudcli.log; ddns-go UI: http://127.0.0.1:9876')
