<#
.SYNOPSIS
  Sprint 0 客户端脚本（局域网版）：在远程操控的电脑上验证服务端可达，并创建桌面快捷方式。
  Bilingual prompts follow the Windows display language (zh* -> Chinese, else English);
  force with -Lang zh|en.

.DESCRIPTION
  架构上客户端 = 浏览器，本机零安装。本脚本只做三件事：
    1. TCP + HTTP 连通性检查（失败时按序给出排查指引）
    2. 在桌面创建 "AI 远程工作台.url" 快捷方式
    3. 用默认浏览器打开（-NoOpen 跳过）

  服务端地址来源（按优先级）：
    a. -Url / -ServerIp 参数（含 install-client.bat 转入的 SERVER_URL）
    b. 上次成功连接的地址（存于本目录 .last-server-url）——回车确认或输入新值
    c. 交互输入；TCP 连通后自动保存，下次运行直接回车即确认

  无需管理员权限；不修改系统配置，可反复运行。

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
    [string]$ShortcutName = 'AI 远程工作台',
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

$ErrorActionPreference = 'Stop'

# ---------- 语言 / 输出辅助 ----------
# 界面语言跟随 Windows 显示语言（zh* -> 中文，其余英文）；-Lang 可强制指定
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }

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
        Write-Step (T '检测到上次使用的服务端地址' 'Found the previously used server URL')
        $typed = Read-Host (T "    直接回车确认 $last ，或输入新地址" "    Press Enter to keep $last , or type a new URL")
        if ($typed -and $typed.Trim()) { $Url = $typed.Trim() } else { $Url = $last }
    }
    else {
        Write-Step (T '首次使用：请提供服务端地址' 'First run: please provide the server URL')
        Write-Host  (T "    地址在服务端运行 install-server.bat 后会打印，形如 http://192.168.1.100:$Port" "    The URL is printed by install-server.bat on the server, like http://192.168.1.100:$Port") -ForegroundColor DarkGray
        $typed = Read-Host (T '    服务端地址（输入一次并连通后将记住，之后回车即确认）' "    Server URL (remembered after first successful connect; later runs just press Enter)")
        if ($typed -and $typed.Trim()) { $Url = $typed.Trim() }
    }
    if (-not $Url) {
        Write-Bad (T '未输入地址，已退出' 'No URL given, exiting')
        exit 1
    }
}
elseif ($ServerIp) { $Url = "http://${ServerIp}:$Port" }

# ---------- 参数归一：支持裸 IP / 缺省端口 ----------
if ($Url -notmatch '^https?://') { $Url = "http://$Url" }
try { $uri = [uri]$Url } catch { Write-Bad (T "URL 无法解析：$Url" "Cannot parse URL: $Url"); exit 1 }
if ($uri.Port -eq 80) {
    # 未显式带端口时按 CloudCLI 默认端口补齐
    $builder = New-Object System.UriBuilder($uri)
    $builder.Port = $Port
    $uri = $builder.Uri
    $Url = $uri.ToString()
}

# ---------- 1. 连通性 ----------
Write-Step (T "检查服务端可达：$($uri.Host):$($uri.Port)" "Checking server reachability: $($uri.Host):$($uri.Port)")
$t = Test-NetConnection -ComputerName $uri.Host -Port $uri.Port -WarningAction SilentlyContinue
if (-not $t.TcpTestSucceeded) {
    Write-Bad (T "TCP $($uri.Host):$($uri.Port) 不可达，按顺序排查（多数在服务端侧）：" "TCP $($uri.Host):$($uri.Port) unreachable. Check in order (mostly server-side):")
    Write-Host  (T '    1) 服务端 cloudcli 是否已启动（窗口保持开启）' "    1) Is cloudcli started on the server? (keep its window open)") -ForegroundColor Yellow
    Write-Host  (T '    2) 服务端防火墙规则 "CloudCLI LAN" 是否存在（可重跑 install-server.ps1）' '    2) Does the "CloudCLI LAN" firewall rule exist? (re-run install-server.ps1)') -ForegroundColor Yellow
    Write-Host  (T '    3) 两台机器是否同一 WiFi / 同网段（别用访客网络）' '    3) Are both machines on the same Wi-Fi / subnet? (avoid guest networks)') -ForegroundColor Yellow
    Write-Host  (T '    4) 路由器是否开启 AP 隔离（设备间互 ping 不通即是）' '    4) Does the router enable AP isolation? (devices cannot ping each other)') -ForegroundColor Yellow
    Write-Host  (T '    详见仓库 docs/research/sprint0-cloudcli-lan-deploy.md §6' '    See docs/research/sprint0-cloudcli-lan-deploy.md §6') -ForegroundColor Yellow
    exit 1
}
Write-Ok (T "TCP $($uri.Host):$($uri.Port) 可达" "TCP $($uri.Host):$($uri.Port) reachable")

# 记住本次地址：下次运行直接回车确认即可
[System.IO.File]::WriteAllText($lastUrlFile, $Url, (New-Object System.Text.UTF8Encoding($false)))
Write-Info (T '已记住该地址（.last-server-url），下次运行回车即确认' 'URL saved (.last-server-url); next run just press Enter to confirm')

try {
    $resp = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 8
    Write-Ok (T "HTTP 响应正常（$($resp.StatusCode) $($resp.StatusDescription)）" "HTTP responds fine ($($resp.StatusCode) $($resp.StatusDescription))")
} catch {
    Write-Info (T "HTTP 探测未通过（$($_.Exception.Message)）——端口通但 Web 服务未就绪，到服务端确认 cloudcli 已启动" "HTTP probe failed ($($_.Exception.Message)) - port open but web service not ready; confirm cloudcli is started on the server")
}

# ---------- 2. 桌面快捷方式 ----------
Write-Step (T '创建桌面快捷方式' 'Creating desktop shortcut')
$desktop  = [Environment]::GetFolderPath('Desktop')
$shortcut = Join-Path $desktop "$ShortcutName.url"
$content  = "[InternetShortcut]`r`nURL=$Url"
[System.IO.File]::WriteAllText($shortcut, $content, (New-Object System.Text.UTF8Encoding($false)))
Write-Ok $shortcut
Write-Info (T '建议：浏览器打开后收藏 / 固定到任务栏，之后双击 .url 即达' 'Tip: bookmark / pin it after opening; later just double-click the .url')

# ---------- 3. 打开浏览器 ----------
if (-not $NoOpen) {
    Write-Step (T '用默认浏览器打开' 'Opening in default browser')
    Start-Process $Url
    Write-Ok (T '已打开（工具开关等设置在页面内操作，配置保存在服务端）' 'Opened (tool toggles etc. live in the web UI; config is stored server-side)')
}

Write-Host ''
Write-Host (T '客户端就绪：本机无需安装任何其他软件，浏览器即客户端。' 'Client is ready: nothing else to install here - the browser is the client.') -ForegroundColor Magenta
