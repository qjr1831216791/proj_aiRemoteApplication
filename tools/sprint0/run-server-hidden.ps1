<#
.SYNOPSIS
  幂等后台启动 CloudCLI：端口 3001 已在监听则直接退出，否则以隐藏窗口拉起。

.DESCRIPTION
  供两处共用：
    1. setup-autostart.ps1 注册的登录计划任务
    2. Claude Code 的 SessionStart hook（docs/research/sprint0-cloudcli-lan-deploy.md）
  特点：立即返回（不阻塞调用方）、可重复执行、进程与调用方解耦。

  日志：%TEMP%\cloudcli.log（含各端访问地址）
  提示语言跟随 Windows 显示语言（zh* -> 中文，其余英文），可 -Lang zh|en 强制。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\run-server-hidden.ps1
#>
[CmdletBinding()]
param(
    [int]$Port = 3001,
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

# 界面语言跟随 Windows 显示语言（zh* -> 中文，其余英文）；-Lang 可强制指定
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }

# 已在运行则无事可做（幂等守卫，hook/任务重复触发时为无操作）
if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { exit 0 }

# 计划任务/hook 环境下 PATH 可能未包含 npm 全局目录，从注册表刷新
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
            [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (Get-Command cloudcli -ErrorAction SilentlyContinue)) { exit 1 }

$log = Join-Path $env:TEMP 'cloudcli.log'

# 各端访问地址记入日志：后台模式没有窗口，这是唯一的查询出口
# （IP 随网络/热点变化，每次实际启动都重新探测）。
# 必须在 Start-Process 之前写入：子进程的重定向会占用日志句柄，之后写入会冲突报错
$ips = Get-NetIPAddress -AddressFamily IPv4 -PrefixOrigin Dhcp,Manual | Where-Object {
    $_.InterfaceAlias -notlike 'vEthernet*' -and
    $_.InterfaceAlias -notlike 'Loopback*'   -and
    $_.InterfaceAlias -notlike 'WSL*'        -and
    $_.IPAddress -match '^(10\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[01])\.)'
} | Select-Object -ExpandProperty IPAddress -Unique
"==== $(Get-Date -Format s) run-server-hidden: starting cloudcli on port $Port ====" | Add-Content -Path $log -Encoding UTF8
$urlLine = "==== URLs $(T '本机' 'Local'): http://localhost:$Port"
foreach ($ip in $ips) { $urlLine += " | $(T '局域网' 'LAN'): http://${ip}:$Port" }
$urlLine | Add-Content -Path $log -Encoding UTF8

# 经 cmd 分离启动：不阻塞本脚本，输出与错误重定向到日志
Start-Process -WindowStyle Hidden -FilePath 'cmd.exe' `
    -ArgumentList "/c cloudcli 1>>`"%TEMP%\cloudcli.log`" 2>&1"
exit 0
