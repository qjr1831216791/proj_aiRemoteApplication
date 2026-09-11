<#
.SYNOPSIS
  一键注册（或移除）CloudCLI HTTPS 栈的开机自启（登录触发计划任务）。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  管理两个组件的登录自启，全部幂等（已在运行则自动跳过）：
    1. CloudCLI        -> run-server-hidden.ps1（隐藏窗口，端口守卫）
    2. Caddy           -> run-caddy-hidden.ps1（注入 .env 凭证后 caddy run --config
                          Caddyfile；必须长驻 run 而非 start，见组件定义处注释与 §9.5-⑩）

  用法（开关）：
    启用：powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1
    关闭：powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -Remove

  变更范围：仅操作名为 "* Sprint0 autostart" 的两个计划任务，不改其他系统配置。
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
$caddyRunner = Join-Path $PSScriptRoot 'run-caddy-hidden.ps1'
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
        # 必须用长驻的 run 而非 start：start 会 fork 子进程后退出，计划任务结束时
        # Windows 会把同作业的子进程一并杀死，导致 443 从未真正起来（§9.5-⑩）
        # 经 run-caddy-hidden.ps1 拉起：先注入栈 .env 的 {env.*} 凭证再前台 run
        # （插件式 Caddyfile，spec 006 / ADR-0003）
        Arg    = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$caddyRunner`" -StackDir `"$StackDir`""
        Check  = (Join-Path $StackDir 'caddy.exe')
        Why    = (T "缺少 $StackDir\caddy.exe" "Missing $StackDir\caddy.exe")
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
Write-Info (T '查看日志：%TEMP%\cloudcli.log' 'Log: %TEMP%\cloudcli.log')
