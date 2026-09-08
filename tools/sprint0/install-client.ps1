<#
.SYNOPSIS
  Sprint 0 客户端脚本（局域网版）：在远程操控的电脑上验证服务端可达，并创建桌面快捷方式。

.DESCRIPTION
  架构上客户端 = 浏览器，本机零安装。本脚本只做三件事：
    1. TCP + HTTP 连通性检查（失败时按序给出排查指引）
    2. 在桌面创建 "AI 远程工作台.url" 快捷方式
    3. 用默认浏览器打开（-NoOpen 跳过）

  服务端地址来源（按优先级）：
    a. -Url / -ServerIp 参数（含 install-client.bat 转入的 SERVER_URL）
    b. 上次成功连接的地址（存于本目录 .last-server-url）——回车确认或输入新值
    c. 交互输入；TCP 连通后自动保存，下次运行直接回车即确认

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\install-client.ps1 -Url http://192.168.1.100:3001
  powershell -ExecutionPolicy Bypass -File .\install-client.ps1 -ServerIp 192.168.1.100
  powershell -ExecutionPolicy Bypass -File .\install-client.ps1 -Url 192.168.1.100 -NoOpen
  powershell -ExecutionPolicy Bypass -File .\install-client.ps1               # 交互式：回车确认上次地址
#>
[CmdletBinding()]
param(
    [string]$Url,
    [string]$ServerIp,
    [int]$Port = 3001,
    [switch]$NoOpen,
    [string]$ShortcutName = 'AI 远程工作台'
)

$ErrorActionPreference = 'Stop'

# ---------- 输出辅助 ----------
function Write-Step { param([string]$Message) Write-Host "`n==> $Message" -ForegroundColor Cyan }
function Write-Ok   { param([string]$Message) Write-Host "    [OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "    [i ] $Message"  -ForegroundColor DarkGray }
function Write-Bad  { param([string]$Message) Write-Host "    [X ] $Message"  -ForegroundColor Red }

# ---------- 地址解析：-Url/-ServerIp 参数 > 上次记录（回车确认）> 交互输入 ----------
$lastUrlFile = Join-Path $PSScriptRoot '.last-server-url'

if (-not $Url -and -not $ServerIp) {
    $last = ''
    if (Test-Path $lastUrlFile) {
        $last = (Get-Content $lastUrlFile -Raw -ErrorAction SilentlyContinue)
        if ($last) { $last = $last.Trim() }
    }
    if ($last) {
        Write-Step '检测到上次使用的服务端地址'
        $typed = Read-Host "    直接回车确认 $last ，或输入新地址"
        if ($typed -and $typed.Trim()) { $Url = $typed.Trim() } else { $Url = $last }
    }
    else {
        Write-Step '首次使用：请提供服务端地址'
        Write-Host  "    地址在服务端运行 install-server.bat 后会打印，形如 http://192.168.1.100:$Port" -ForegroundColor DarkGray
        $typed = Read-Host '    服务端地址（输入一次并连通后将记住，之后回车即确认）'
        if ($typed -and $typed.Trim()) { $Url = $typed.Trim() }
    }
    if (-not $Url) {
        Write-Bad '未输入地址，已退出'
        exit 1
    }
}
elseif ($ServerIp) { $Url = "http://${ServerIp}:$Port" }
if ($Url -notmatch '^https?://') { $Url = "http://$Url" }
try { $uri = [uri]$Url } catch { Write-Bad "URL 无法解析：$Url"; exit 1 }
if ($uri.Port -eq 80) {
    # 未显式带端口时按 CloudCLI 默认端口补齐
    $builder = New-Object System.UriBuilder($uri)
    $builder.Port = $Port
    $uri = $builder.Uri
    $Url = $uri.ToString()
}

# ---------- 1. 连通性 ----------
Write-Step "检查服务端可达：$($uri.Host):$($uri.Port)"
$t = Test-NetConnection -ComputerName $uri.Host -Port $uri.Port -WarningAction SilentlyContinue
if (-not $t.TcpTestSucceeded) {
    Write-Bad "TCP $($uri.Host):$($uri.Port) 不可达，按顺序排查（多数在服务端侧）："
    Write-Host  '    1) 服务端 cloudcli 是否已启动（窗口保持开启）' -ForegroundColor Yellow
    Write-Host  '    2) 服务端防火墙规则 "CloudCLI LAN" 是否存在（可重跑 install-server.ps1）' -ForegroundColor Yellow
    Write-Host  '    3) 两台机器是否同一 WiFi / 同网段（别用访客网络）' -ForegroundColor Yellow
    Write-Host  '    4) 路由器是否开启 AP 隔离（设备间互 ping 不通即是）' -ForegroundColor Yellow
    Write-Host  '    详见仓库 docs/research/sprint0-cloudcli-lan-deploy.md §6' -ForegroundColor Yellow
    exit 1
}
Write-Ok "TCP $($uri.Host):$($uri.Port) 可达"

# 记住本次地址：下次运行直接回车确认即可
[System.IO.File]::WriteAllText($lastUrlFile, $Url, (New-Object System.Text.UTF8Encoding($false)))
Write-Info '已记住该地址（.last-server-url），下次运行回车即确认'

try {
    $resp = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 8
    Write-Ok "HTTP 响应正常（$($resp.StatusCode) $($resp.StatusDescription)）"
} catch {
    Write-Info "HTTP 探测未通过（$($_.Exception.Message)）——端口通但 Web 服务未就绪，到服务端确认 cloudcli 已启动"
}

# ---------- 2. 桌面快捷方式 ----------
Write-Step '创建桌面快捷方式'
$desktop  = [Environment]::GetFolderPath('Desktop')
$shortcut = Join-Path $desktop "$ShortcutName.url"
$content  = "[InternetShortcut]`r`nURL=$Url"
[System.IO.File]::WriteAllText($shortcut, $content, (New-Object System.Text.UTF8Encoding($false)))
Write-Ok $shortcut
Write-Info '建议：浏览器打开后收藏 / 固定到任务栏，之后双击 .url 即达'

# ---------- 3. 打开浏览器 ----------
if (-not $NoOpen) {
    Write-Step '用默认浏览器打开'
    Start-Process $Url
    Write-Ok '已打开（工具开关等设置在页面内操作，配置保存在服务端）'
}

Write-Host ''
Write-Host '客户端就绪：本机无需安装任何其他软件，浏览器即客户端。' -ForegroundColor Magenta
