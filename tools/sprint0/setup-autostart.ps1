<#
.SYNOPSIS
  注册（或移除）CloudCLI 登录自启计划任务 —— Sprint 0 服务端推荐的常驻方式。

.DESCRIPTION
  在当前用户登录时，经 run-server-hidden.ps1 以隐藏窗口启动 CloudCLI（幂等：
  端口已监听则跳过）。日志见 %TEMP%\cloudcli.log。

  变更范围与回滚：仅操作名为 "CloudCLI Sprint0 autostart" 的计划任务，
  不改其他任何系统配置；运行 -Remove 即完全移除。

  关键参数说明：
    -ExecutionTimeLimit Zero：取消默认 72 小时强制结束（服务需要常驻）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1          # 注册
  powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -Remove  # 移除

  注册后立即启动一次（不等重新登录）：
  powershell -NoProfile -ExecutionPolicy Bypass -File .\run-server-hidden.ps1
#>
[CmdletBinding()]
param(
    [switch]$Remove
)

$taskName = 'CloudCLI Sprint0 autostart'

# ---------- 移除模式 ----------
if ($Remove) {
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
        Write-Host "[OK] 已移除计划任务：$taskName"
    } else {
        Write-Host "[i ] 计划任务不存在，无需移除"
    }
    exit 0
}

# ---------- 注册模式 ----------
$runner = Join-Path $PSScriptRoot 'run-server-hidden.ps1'
if (-not (Test-Path $runner)) {
    Write-Host "[X ] 缺少依赖文件：$runner（需与脚本同目录）"
    exit 1
}

$action = New-ScheduledTaskAction -Execute 'powershell.exe' `
    -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$runner`""
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
# Zero = 不限执行时长（默认 72h 会把常驻服务杀掉）；电池供电时也允许运行
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit ([TimeSpan]::Zero)

try {
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings | Out-Null
} catch {
    Write-Host "[X ] 注册失败：$($_.Exception.Message)"
    Write-Host "     若为权限问题，请以管理员身份重试"
    exit 1
}

Write-Host "[OK] 已注册登录自启：$taskName"
Write-Host '     生效时机：下次登录自动启动；想立即启动执行：'
Write-Host "     powershell -NoProfile -ExecutionPolicy Bypass -File `"$runner`""
Write-Host '     查看日志：%TEMP%\cloudcli.log'
Write-Host '     移除自启：powershell -ExecutionPolicy Bypass -File .\setup-autostart.ps1 -Remove'
