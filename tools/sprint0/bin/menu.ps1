<#
.SYNOPSIS
  Sprint 0 总控菜单：输入选项完成全部功能（menu.bat 的实际实现）。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  汇总服务端全部常用操作：启动/停止服务、状态与地址、安装、HTTPS 配置、
  客户端配置、开机自启开关、ddns-go 管理页。需要管理员的项会自动弹 UAC
  并在新的管理员窗口中执行。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\menu.ps1
#>
[CmdletBinding()]
param(
    [string]$StackDir = 'D:\Software\cloudcli-https',
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

# ---------- 语言 ----------
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }

$port = 3001
$httpsPort = 443

function Get-LanIps {
    (Get-NetIPAddress -AddressFamily IPv4 -PrefixOrigin Dhcp,Manual -ErrorAction SilentlyContinue | Where-Object {
        $_.InterfaceAlias -notlike 'vEthernet*' -and
        $_.InterfaceAlias -notlike 'Loopback*'   -and
        $_.InterfaceAlias -notlike 'WSL*'        -and
        $_.IPAddress -match '^(10\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[01])\.)'
    } | Select-Object -ExpandProperty IPAddress -Unique)
}

function Test-PortListening {
    param([int]$P)
    [bool](Get-NetTCPConnection -LocalPort $P -State Listen -ErrorAction SilentlyContinue)
}

function Show-Status {
    $lan = if (Test-PortListening $port) { (T '运行中' 'running') } else { (T '未运行' 'not running') }
    $https = if (Test-PortListening $httpsPort) { (T '运行中' 'running') } else { (T '未运行' 'not running') }
    Write-Host ''
    Write-Host (T '  CloudCLI  : ' '  CloudCLI  : ') -NoNewline; Write-Host $lan -ForegroundColor $(if (Test-PortListening $port) { 'Green' } else { 'Red' }) -NoNewline
    Write-Host (T "   http://localhost:$port" "   http://localhost:$port") -ForegroundColor DarkGray
    Write-Host (T '  HTTPS/Caddy: ' '  HTTPS/Caddy: ') -NoNewline; Write-Host $https -ForegroundColor $(if (Test-PortListening $httpsPort) { 'Green' } else { 'Red' }) -NoNewline
    Write-Host (T '   https://ai.jackqi.cn' '   https://ai.jackqi.cn') -ForegroundColor DarkGray
    Write-Host ''
}

function Invoke-Elevated {
    param([string]$File)
    try {
        Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$File`""
        Write-Host (T '    已弹出管理员窗口，请在那个窗口中查看结果。' '    An elevated window has opened; check results there.') -ForegroundColor DarkGray
    } catch {
        Write-Host (T '    已取消或提权失败。' '    Cancelled or elevation failed.') -ForegroundColor Yellow
    }
}

function Show-Menu {
    Clear-Host
    Write-Host '=========================================' -ForegroundColor Cyan
    Write-Host (T '     Sprint 0 AI 远程工作台 - 总控菜单' '     Sprint 0 AI Workbench - Main Menu') -ForegroundColor Cyan
    Write-Host '=========================================' -ForegroundColor Cyan
    Show-Status
    Write-Host (T '  1. 启动服务（后台）并打开本机页面' '  1. Start services (background) and open local page')
    Write-Host (T '  2. 停止服务' '  2. Stop services')
    Write-Host (T '  3. 查看各端访问地址' '  3. Show access URLs for every device')
    Write-Host (T '  4. 安装/重装服务端（管理员）' '  4. Install/reinstall server (admin)')
    Write-Host (T '  5. HTTPS 环境配置：防火墙/专用网络/hosts（管理员）' '  5. HTTPS setup: firewall/private network/hosts (admin)')
    Write-Host (T '  6. 客户端配置（本机验证 + 桌面快捷方式）' '  6. Client setup (verify + desktop shortcut)')
    Write-Host (T '  7. 开机自启：全部开启' '  7. Autostart: enable all')
    Write-Host (T '  8. 开机自启：全部关闭' '  8. Autostart: disable all')
    Write-Host (T '  9. 打开 ddns-go 管理页' '  9. Open ddns-go admin page')
    Write-Host (T '  0. 退出' '  0. Exit')
    Write-Host ''
    Write-Host (T '请输入选项: ' 'Choose an option: ') -NoNewline -ForegroundColor Yellow
}

while ($true) {
    Show-Menu
    $choice = Read-Host
    if ($null -eq $choice) { exit 0 }   # 输入流结束（EOF）时退出，避免死循环
    switch ($choice.Trim()) {
        '1' {
            powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'run-server-hidden.ps1')
            Start-Process "http://localhost:$port"
            Write-Host (T '[OK] 服务已启动（后台），浏览器已打开本机页面。' '[OK] Services started (background); opened local page in browser.') -ForegroundColor Green
        }
        '2' {
            powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'stop-server.ps1')
        }
        '3' {
            Show-Status
            Write-Host (T '  本机访问:  http://localhost:3001' '  This PC:  http://localhost:3001')
            foreach ($ip in (Get-LanIps)) {
                Write-Host (T "  局域网:    http://${ip}:3001   |   https://ai.jackqi.cn（推荐）" "  LAN:      http://${ip}:3001   |   https://ai.jackqi.cn (recommended)")
            }
        }
        '4' {
            Invoke-Elevated (Join-Path $PSScriptRoot 'install-server.ps1')
        }
        '5' {
            Invoke-Elevated (Join-Path $PSScriptRoot 'enable-https.ps1')
        }
        '6' {
            powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'install-client.ps1')
        }
        '7' {
            powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'setup-autostart.ps1')
        }
        '8' {
            powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'setup-autostart.ps1') -Remove
        }
        '9' {
            Start-Process 'http://127.0.0.1:9876'
            Write-Host (T '[OK] 已打开 ddns-go 管理页（如打不开，说明 ddns-go 未运行，先选 1）。' '[OK] Opened ddns-go page (if it fails, ddns-go is not running - choose 1 first).') -ForegroundColor Green
        }
        '0' {
            exit 0
        }
        default {
            Write-Host (T '[!] 无效选项，请输入菜单中的数字。' '[!] Invalid option, enter a number from the menu.') -ForegroundColor Yellow
        }
    }
    Write-Host ''
    Read-Host (T '按回车返回菜单...' 'Press Enter to return to menu...') | Out-Null
}
