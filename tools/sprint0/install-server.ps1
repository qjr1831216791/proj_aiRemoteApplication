<#
.SYNOPSIS
  Sprint 0 服务端一键安装（局域网版）：在运行 AI 实例的 Windows 计算机上部署 CloudCLI。

.DESCRIPTION
  自动覆盖 docs/research/sprint0-cloudcli-lan-deploy.md 的 §0 前置检查、§1 安装、
  §3.2 防火墙放行、§3.3 IP 提示、§4 电源常开。

  执行内容（均可逆，回滚命令见部署文档 §8）：
    1. 检查/安装 Node.js >= 20（缺失时经 winget 安装 OpenJS.NodeJS.LTS）
    2. 检查 Claude Code（缺失即退出——CC Switch 前置必须手动完成）
    3. 安装 @cloudcli-ai/cloudcli（已安装则跳过，仅刷新其余配置；
       -Update 强制升级最新版；-UseMirror 可换国内镜像）
    4. powercfg 设置插电永不睡眠（standby-timeout-ac 0）
    5. 创建防火墙入站规则 "CloudCLI LAN <port>"（仅专用网络 Private）
    6. 将当前网络配置文件设为"专用"（域网络跳过）
    7. 探测局域网 IPv4，打印客户端访问地址

  幂等设计：装好后重复运行 = 跳过已装组件、只刷新配置，不会重复安装。
  需要管理员权限（防火墙 / 电源 / 网络配置文件）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\install-server.ps1
  powershell -ExecutionPolicy Bypass -File .\install-server.ps1 -UseMirror -Port 3001
  powershell -ExecutionPolicy Bypass -File .\install-server.ps1 -Update   # 升级 CloudCLI 到最新

.NOTES
  决策依据：docs/research/remote-solutions.md §7（D0 先试用 / Q1 宿主=开发机 / Q2 Tailscale）。
#>
[CmdletBinding()]
param(
    [int]$Port = 3001,
    [switch]$UseMirror,
    [switch]$Update
)

$ErrorActionPreference = 'Stop'

# ---------- 输出辅助 ----------
function Write-Step { param([string]$Message) Write-Host "`n==> $Message" -ForegroundColor Cyan }
function Write-Ok   { param([string]$Message) Write-Host "    [OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "    [i ] $Message"  -ForegroundColor DarkGray }
function Write-Bad  { param([string]$Message) Write-Host "    [X ] $Message"  -ForegroundColor Red }

function Refresh-Path {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
                [Environment]::GetEnvironmentVariable('Path', 'User')
}

# ---------- 0. 管理员检查 ----------
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Bad '请以管理员身份运行：先打开管理员 PowerShell，再执行'
    Write-Host  '    powershell -ExecutionPolicy Bypass -File .\install-server.ps1' -ForegroundColor Yellow
    exit 1
}

# ---------- 1. Node.js ----------
Write-Step '步骤 1/7：检查 Node.js（要求 >= 20）'
$nodeOk = $false
if (Get-Command node -ErrorAction SilentlyContinue) {
    $raw = (node --version).TrimStart('v')
    try {
        if ([version]$raw -ge [version]'20.0.0') { Write-Ok "Node.js $raw 已安装"; $nodeOk = $true }
        else { Write-Bad "Node.js $raw 低于 20，需要安装/升级" }
    } catch { Write-Info "无法解析版本号 $raw，视为不满足" }
} else { Write-Info '未检测到 node' }

if (-not $nodeOk) {
    if (Get-Command winget -ErrorAction SilentlyContinue) {
        Write-Info '经 winget 安装 Node.js LTS ...'
        winget install OpenJS.NodeJS.LTS --accept-package-agreements --accept-source-agreements
        Refresh-Path
        if (Get-Command node -ErrorAction SilentlyContinue) {
            Write-Ok "Node.js $(node --version) 安装成功"
        } else {
            Write-Bad 'Node 已安装但当前会话不可见：请关闭窗口，重新以管理员身份运行本脚本一次'
            exit 1
        }
    } else {
        Write-Bad '未检测到 winget：请手动安装 Node.js LTS（https://nodejs.org/）后重跑本脚本'
        exit 1
    }
}

# ---------- 2. Claude Code ----------
Write-Step '步骤 2/7：检查 Claude Code（CC Switch 前置）'
if (Get-Command claude -ErrorAction SilentlyContinue) {
    Write-Ok "claude 已安装：$(claude --version)"
} else {
    Write-Bad '未检测到 claude 命令，请先手动完成：'
    Write-Host  '    1) 安装 Claude Code；2) 用 CC Switch 配置国产供应商；' -ForegroundColor Yellow
    Write-Host  '    3) 终端运行 claude 确认可正常对话，然后重跑本脚本' -ForegroundColor Yellow
    exit 1
}

# ---------- 3. 安装/升级 CloudCLI（幂等：已装则跳过）----------
Write-Step '步骤 3/7：安装 CloudCLI（已装则跳过，-Update 升级）'
if ($UseMirror) {
    npm config set registry https://registry.npmmirror.com
    Write-Ok 'npm registry -> https://registry.npmmirror.com（国内镜像）'
}
$cloudcliInstalled = Get-Command cloudcli -ErrorAction SilentlyContinue
if (-not $cloudcliInstalled) {
    Refresh-Path
    $cloudcliInstalled = Get-Command cloudcli -ErrorAction SilentlyContinue
}

if ($cloudcliInstalled -and -not $Update) {
    Write-Ok 'cloudcli 已安装，跳过安装（升级：加 -Update 重跑）'
} else {
    $pkg = if ($Update) { '@cloudcli-ai/cloudcli@latest' } else { '@cloudcli-ai/cloudcli' }
    npm install -g $pkg
    if ($LASTEXITCODE -ne 0) {
        Write-Bad "npm install 失败（exit $LASTEXITCODE）；网络慢可加 -UseMirror 重试"
        exit 1
    }
    if (-not (Get-Command cloudcli -ErrorAction SilentlyContinue)) { Refresh-Path }
    if (Get-Command cloudcli -ErrorAction SilentlyContinue) {
        Write-Ok 'cloudcli 命令已可用'
    } else {
        Write-Info 'cloudcli 已安装但当前会话 PATH 未刷新，新开一个终端即可使用'
    }
}

# ---------- 4. 电源常开 ----------
Write-Step '步骤 4/7：电源设置（插电状态永不睡眠）'
powercfg /change standby-timeout-ac 0
Write-Ok 'standby-timeout-ac = 0（屏幕自动关闭不受影响）'

# ---------- 5. 防火墙 ----------
Write-Step "步骤 5/7：防火墙放行 TCP $Port（仅专用网络）"
$ruleName = "CloudCLI LAN $Port"
if (Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue) {
    Write-Ok "入站规则已存在：$ruleName（跳过）"
} else {
    New-NetFirewallRule -DisplayName $ruleName -Direction Inbound -Protocol TCP `
        -LocalPort $Port -Action Allow -Profile Private | Out-Null
    Write-Ok "已创建入站规则：$ruleName"
}

# ---------- 6. 网络配置文件 ----------
Write-Step '步骤 6/7：将网络配置文件设为"专用"'
foreach ($p in Get-NetConnectionProfile) {
    if ($p.NetworkCategory -eq 'DomainAuthenticated') {
        Write-Info "$($p.Name) 为域网络，跳过"
    } elseif ($p.NetworkCategory -ne 'Private') {
        Set-NetConnectionProfile -InterfaceIndex $p.InterfaceIndex -NetworkCategory Private
        Write-Ok "$($p.Name) -> Private"
    } else {
        Write-Ok "$($p.Name) 已是 Private"
    }
}

# ---------- 7. 局域网 IP ----------
Write-Step '步骤 7/7：探测局域网 IPv4'
$privateRx = '^(10\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[01])\.)'
$ips = Get-NetIPAddress -AddressFamily IPv4 -PrefixOrigin Dhcp,Manual | Where-Object {
    $_.InterfaceAlias -notlike 'vEthernet*' -and
    $_.InterfaceAlias -notlike 'Loopback*'   -and
    $_.InterfaceAlias -notlike 'WSL*'        -and
    $_.IPAddress -match $privateRx
} | Select-Object -ExpandProperty IPAddress -Unique

if (-not $ips) {
    Write-Info '未自动识别出局域网 IPv4，请手动执行 ipconfig 查看（通常为 192.168.x.x）'
} else {
    foreach ($ip in $ips) { Write-Ok "客户端访问地址：http://${ip}:$Port" }
    if ($ips.Count -gt 1) { Write-Info '多个地址时，选与客户端同一网段的那个（通常 192.168.x.x）' }
}

# ---------- 汇总 ----------
Write-Host ''
Write-Host '==================== 服务端安装完成 ====================' -ForegroundColor Magenta
Write-Host '  启动服务：cloudcli          （默认监听 3001，窗口保持开启）'
Write-Host "  本机访问：http://localhost:$Port"
Write-Host '  手动收尾（脚本覆盖不到的两步）：'
Write-Host '    1) 浏览器打开上面的地址 -> 设置 -> 开启需要的工具（默认全禁用）'
Write-Host '    2) 确认 CC Switch 当前供应商可用（终端跑一次 claude）'
Write-Host '  客户端电脑：powershell -ExecutionPolicy Bypass -File .\install-client.ps1 -Url http://<上面的地址>'
Write-Host '  手机：同 WiFi 浏览器直接打开同一地址（Chrome 可"添加到主屏幕"）'
Write-Host '  排障：docs/research/sprint0-cloudcli-lan-deploy.md §6'
Write-Host '=======================================================' -ForegroundColor Magenta
