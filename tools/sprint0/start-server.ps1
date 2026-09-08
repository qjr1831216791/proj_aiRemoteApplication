<#
.SYNOPSIS
  前台启动 CloudCLI 并打印各端访问地址（start-server.bat 的实际实现）。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  端口已在监听则仅打开浏览器后退出；否则在本控制台前台运行 cloudcli——
  关闭窗口（或 Ctrl+C）即停止服务。后台常驻请用 run-server-hidden.ps1。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\start-server.ps1
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

# ---------- 端口状态 + 各端访问地址 ----------
$listening = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
Write-Host ''
Write-Host (T '============== 各端访问地址 ==============' '============== Access URLs ==============')
Write-Host "  $(T '本机' 'This PC')`: http://localhost:$Port"
$ips = Get-NetIPAddress -AddressFamily IPv4 -PrefixOrigin Dhcp,Manual | Where-Object {
    $_.InterfaceAlias -notlike 'vEthernet*' -and
    $_.InterfaceAlias -notlike 'Loopback*'   -and
    $_.InterfaceAlias -notlike 'WSL*'        -and
    $_.IPAddress -match '^(10\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[01])\.)'
} | Select-Object -ExpandProperty IPAddress -Unique
foreach ($ip in $ips) {
    Write-Host "  $(T '移动端/其他电脑' 'Phone/other PCs')`: http://${ip}:$Port"
}
Write-Host '========================================='
Write-Host ''

# ---------- 已在运行：仅打开浏览器 ----------
if ($listening) {
    Write-Host (T "CloudCLI 已在运行（端口 $Port）- 打开浏览器 ..." "CloudCLI is already running on port $Port - opening browser ...")
    Start-Process "http://localhost:$Port"
    Start-Sleep -Seconds 3
    exit 0
}

# ---------- 前台启动 ----------
Write-Host (T "正在端口 $Port 上启动 CloudCLI ..." "Starting CloudCLI server on port $Port ...")
Write-Host (T '保持本窗口开启，按 Ctrl+C 停止服务。' 'Keep this window OPEN. Press Ctrl+C to stop.')
Write-Host ''
& cloudcli

Write-Host ''
Write-Host (T 'CloudCLI 已退出（或被 Ctrl+C 停止）。以上信息供排查。' 'CloudCLI exited (or stopped by Ctrl+C). See messages above.')
