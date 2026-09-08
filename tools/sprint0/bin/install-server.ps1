<#
.SYNOPSIS
  Sprint 0 服务端一键安装（局域网版）：在运行 AI 实例的 Windows 计算机上部署 CloudCLI。
  Bilingual prompts follow the Windows display language (zh* -> Chinese, else English);
  force with -Lang zh|en.

.DESCRIPTION
  自动覆盖 docs/research/sprint0-cloudcli-lan-deploy.md 的 §0 前置检查、§1 安装、
  §3.2 防火墙放行、§3.3 IP 提示、§4 电源常开。

  执行内容（均可逆，回滚命令见部署文档 §8）：
    1. 检查/安装 Node.js >= 20（缺失时经 winget 安装 OpenJS.NodeJS.LTS）
    2. 检查 Claude Code（缺失即退出——CC Switch 前置必须手动完成）
    3. 镜像源体检：检测已停服的旧淘宝源（npm.taobao.org），经确认自动迁移
       npmmirror；随后安装 @cloudcli-ai/cloudcli（已安装则跳过，
       -Update 强制升级最新版；-UseMirror 可换国内镜像）
    4. powercfg 设置插电永不睡眠（standby-timeout-ac 0）
    5. 创建防火墙入站规则 "CloudCLI LAN <port>"（已存在则跳过）
    6. 将当前网络配置文件设为"专用"（域网络跳过，已是专用则跳过）
    7. 探测局域网 IPv4，打印客户端访问地址

  幂等设计：装好后重复运行 = 跳过已装组件、只刷新配置，不会重复安装。
  需要管理员权限（防火墙 / 电源 / 网络配置文件）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\install-server.ps1
  powershell -ExecutionPolicy Bypass -File .\install-server.ps1 -UseMirror -Port 3001
  powershell -ExecutionPolicy Bypass -File .\install-server.ps1 -Update   # 升级 CloudCLI 到最新
#>
[CmdletBinding()]
param(
    [int]$Port = 3001,
    [switch]$UseMirror,
    [switch]$Update,
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

function Refresh-Path {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
                [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Repair-DeadMirror {
    # 镜像源体检：旧淘宝源 npm.taobao.org 已停服（证书失效），会导致原生模块
    # （如 better-sqlite3）经 node-gyp 编译时拉取头文件失败。检测到即提示，
    # 经用户确认后迁移到官方继任镜像 npmmirror（持久化 + 当前会话立即生效）。
    $dead = 'npm.taobao.org'
    $map = @{
        'NODEJS_ORG_MIRROR'      = 'https://npmmirror.com/mirrors/node/'
        'NVM_NODEJS_ORG_MIRROR'  = 'https://npmmirror.com/mirrors/node/'
        'NVMW_NODEJS_ORG_MIRROR' = 'https://npmmirror.com/mirrors/node/'
        'NODIST_NODE_MIRROR'     = 'https://npmmirror.com/mirrors/node/'
        'IOJS_ORG_MIRROR'        = 'https://npmmirror.com/mirrors/iojs/'
        'NVM_IOJS_ORG_MIRROR'    = 'https://npmmirror.com/mirrors/iojs/'
        'NVMW_IOJS_ORG_MIRROR'   = 'https://npmmirror.com/mirrors/iojs/'
        'NODIST_IOJS_MIRROR'     = 'https://npmmirror.com/mirrors/iojs/'
        'NVMW_NPM_MIRROR'        = 'https://npmmirror.com/mirrors/npm/'
    }

    $staleEnv = @()
    foreach ($name in $map.Keys) {
        foreach ($scope in 'User', 'Machine', 'Process') {
            $v = [Environment]::GetEnvironmentVariable($name, $scope)
            if ($v -and $v -match $dead) { $staleEnv += $name; break }
        }
    }
    $staleNpm = @()
    foreach ($key in 'registry', 'disturl') {
        $v = (npm config get $key)
        if ($v -and $v -notmatch '^(undefined|null)$' -and $v -match $dead) { $staleNpm += $key }
    }
    $npmrc = Join-Path $env:USERPROFILE '.npmrc'
    if (Test-Path $npmrc) {
        foreach ($line in (Get-Content $npmrc)) {
            if ($line -match '^\s*(registry|disturl)\s*=.*npm\.taobao\.org') {
                if ($staleNpm -notcontains $Matches[1]) { $staleNpm += $Matches[1] }
            }
        }
    }
    if ($staleEnv.Count -eq 0 -and $staleNpm.Count -eq 0) { return }

    Write-Bad (T "检测到已停服的旧淘宝镜像源（$dead）：" "Detected the long-dead legacy Taobao mirror ($dead):")
    foreach ($name in $staleEnv) {
        Write-Host (T "    环境变量 $name = $([Environment]::GetEnvironmentVariable($name, 'Process'))" "    env var $name = $([Environment]::GetEnvironmentVariable($name, 'Process'))") -ForegroundColor Yellow
    }
    foreach ($key in $staleNpm) {
        Write-Host (T "    npm 配置 $key = $(npm config get $key)" "    npm config $key = $(npm config get $key)") -ForegroundColor Yellow
    }
    Write-Host (T '    它会导致依赖原生模块（如 better-sqlite3）安装/编译失败。' '    It breaks install/compile of native modules (e.g. better-sqlite3).') -ForegroundColor Yellow
    $ans = Read-Host (T '    是否自动迁移到 npmmirror（官方继任镜像）？[Y/n]' '    Migrate to npmmirror (the official successor mirror) now? [Y/n]')
    if ($ans -and $ans -notmatch '^[Yy]') {
        Write-Info (T '已保留旧配置；若后续安装失败，请手动更换镜像源后重跑' 'Kept old config; if install fails, switch the mirror manually and re-run')
        return
    }

    foreach ($name in $staleEnv) {
        $newVal = $map[$name]
        foreach ($scope in 'User', 'Machine') {
            $v = [Environment]::GetEnvironmentVariable($name, $scope)
            if ($v -and $v -match $dead) {
                [Environment]::SetEnvironmentVariable($name, $newVal, $scope)
            }
        }
        Set-Item -Path "env:$name" -Value $newVal   # 当前会话立即生效
        Write-Ok "$name -> $newVal"
    }
    foreach ($key in $staleNpm) {
        $newVal = if ($key -eq 'registry') { 'https://registry.npmmirror.com' } else { 'https://npmmirror.com/mirrors/node/' }
        # stderr 重定向放在 cmd 内部：本脚本 EAP=Stop，PS 侧重定向会把原生命令
        # stderr 变成终止错误；新版 npm 也会拒绝写入部分键（如 disturl）
        cmd /c "npm config set $key $newVal 2>nul"
        if ($LASTEXITCODE -ne 0) {
            # 新版 npm 拒绝经 config set 写入部分键（如 disturl），直接维护用户 .npmrc
            $lines = @(Get-Content $npmrc -ErrorAction SilentlyContinue |
                Where-Object { $_ -notmatch "^\s*$([regex]::Escape($key))\s*=" })
            $lines += "$key=$newVal"
            Set-Content -Path $npmrc -Value $lines -Encoding ascii
            Write-Info (T "npm config set 不支持 $key，已直接写入 .npmrc" "npm config set rejects '$key'; wrote it into .npmrc directly")
        }
        Write-Ok "npm $key -> $newVal"
    }
}

# ---------- 0. 管理员检查 ----------
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Bad (T '请以管理员身份运行：先打开管理员 PowerShell，再执行' 'Please run as administrator: open an elevated PowerShell first, then run')
    Write-Host  '    powershell -ExecutionPolicy Bypass -File .\install-server.ps1' -ForegroundColor Yellow
    exit 1
}

# ---------- 1. Node.js ----------
Write-Step (T '步骤 1/7：检查 Node.js（要求 >= 20）' 'Step 1/7: Checking Node.js (>= 20 required)')
$nodeOk = $false
if (Get-Command node -ErrorAction SilentlyContinue) {
    $raw = (node --version).TrimStart('v')
    try {
        if ([version]$raw -ge [version]'20.0.0') { Write-Ok (T "Node.js $raw 已安装" "Node.js $raw already installed"); $nodeOk = $true }
        else { Write-Bad (T "Node.js $raw 低于 20，需要安装/升级" "Node.js $raw is below 20, needs install/upgrade") }
    } catch { Write-Info (T "无法解析版本号 $raw，视为不满足" "Cannot parse version '$raw', treating as not satisfied") }
} else { Write-Info (T '未检测到 node' 'node not found') }

if (-not $nodeOk) {
    if (Get-Command winget -ErrorAction SilentlyContinue) {
        Write-Info (T '经 winget 安装 Node.js LTS ...' 'Installing Node.js LTS via winget ...')
        winget install OpenJS.NodeJS.LTS --accept-package-agreements --accept-source-agreements
        Refresh-Path
        if (Get-Command node -ErrorAction SilentlyContinue) {
            Write-Ok (T "Node.js $(node --version) 安装成功" "Node.js $(node --version) installed")
        } else {
            Write-Bad (T 'Node 已安装但当前会话不可见：请关闭窗口，重新以管理员身份运行本脚本一次' 'Node installed but not visible in this session: close this window and re-run this script as administrator once')
            exit 1
        }
    } else {
        Write-Bad (T '未检测到 winget：请手动安装 Node.js LTS（https://nodejs.org/）后重跑本脚本' 'winget not found: install Node.js LTS manually (https://nodejs.org/) and re-run')
        exit 1
    }
}

# ---------- 2. Claude Code ----------
Write-Step (T '步骤 2/7：检查 Claude Code（CC Switch 前置）' 'Step 2/7: Checking Claude Code (required by CC Switch)')
if (Get-Command claude -ErrorAction SilentlyContinue) {
    Write-Ok (T "claude 已安装：$(claude --version)" "claude already installed: $(claude --version)")
} else {
    Write-Bad (T '未检测到 claude 命令，请先手动完成：' "claude command not found. Finish these manually first:")
    Write-Host  (T '    1) 安装 Claude Code；2) 用 CC Switch 配置国产供应商；' "    1) Install Claude Code; 2) configure your provider in CC Switch;") -ForegroundColor Yellow
    Write-Host  (T '    3) 终端运行 claude 确认可正常对话，然后重跑本脚本' "    3) verify 'claude' chats fine in a terminal, then re-run this script") -ForegroundColor Yellow
    exit 1
}

# ---------- 3. 安装/升级 CloudCLI（幂等：已装则跳过）----------
Write-Step (T '步骤 3/7：安装 CloudCLI（已装则跳过，-Update 升级）' 'Step 3/7: Installing CloudCLI (skipped if present; -Update to upgrade)')
Repair-DeadMirror
if ($UseMirror) {
    npm config set registry https://registry.npmmirror.com
    Write-Ok (T 'npm registry -> https://registry.npmmirror.com（国内镜像）' 'npm registry -> https://registry.npmmirror.com (China mirror)')
}
$cloudcliInstalled = Get-Command cloudcli -ErrorAction SilentlyContinue
if (-not $cloudcliInstalled) {
    Refresh-Path
    $cloudcliInstalled = Get-Command cloudcli -ErrorAction SilentlyContinue
}

if ($cloudcliInstalled -and -not $Update) {
    Write-Ok (T 'cloudcli 已安装，跳过安装（升级：加 -Update 重跑）' 'cloudcli already installed, skipping (to upgrade: re-run with -Update)')
} else {
    $pkg = if ($Update) { '@cloudcli-ai/cloudcli@latest' } else { '@cloudcli-ai/cloudcli' }
    npm install -g $pkg
    if ($LASTEXITCODE -ne 0) {
        Write-Bad (T "npm install 失败（exit $LASTEXITCODE）；网络慢可加 -UseMirror 重试" "npm install failed (exit $LASTEXITCODE); retry with -UseMirror on slow networks")
        Write-Info (T '若报 npm.taobao.org / 证书错误：重跑本脚本，步骤 3 会检测并提示一键迁移镜像源' "If you see npm.taobao.org / certificate errors: re-run this script, step 3 will offer one-key mirror migration")
        exit 1
    }
    if (-not (Get-Command cloudcli -ErrorAction SilentlyContinue)) { Refresh-Path }
    if (Get-Command cloudcli -ErrorAction SilentlyContinue) {
        Write-Ok (T 'cloudcli 命令已可用' 'cloudcli command is available')
    } else {
        Write-Info (T 'cloudcli 已安装但当前会话 PATH 未刷新，新开一个终端即可使用' 'cloudcli installed but PATH not refreshed in this session; open a new terminal to use it')
    }
}

# ---------- 4. 电源常开 ----------
Write-Step (T '步骤 4/7：电源设置（插电状态永不睡眠）' 'Step 4/7: Power settings (never sleep on AC power)')
powercfg /change standby-timeout-ac 0
Write-Ok (T 'standby-timeout-ac = 0（屏幕自动关闭不受影响）' 'standby-timeout-ac = 0 (display auto-off is not affected)')

# ---------- 5. 防火墙 ----------
Write-Step (T "步骤 5/7：防火墙放行 TCP $Port（仅专用网络）" "Step 5/7: Firewall allow TCP $Port (private networks only)")
$ruleName = "CloudCLI LAN $Port"
if (Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue) {
    Write-Ok (T "入站规则已存在：$ruleName（跳过）" "Inbound rule already exists: $ruleName (skipped)")
} else {
    New-NetFirewallRule -DisplayName $ruleName -Direction Inbound -Protocol TCP `
        -LocalPort $Port -Action Allow -Profile Private | Out-Null
    Write-Ok (T "已创建入站规则：$ruleName" "Inbound rule created: $ruleName")
}

# ---------- 6. 网络配置文件 ----------
Write-Step (T '步骤 6/7：将网络配置文件设为"专用"' 'Step 6/7: Setting network profile to "Private"')
foreach ($p in Get-NetConnectionProfile) {
    if ($p.NetworkCategory -eq 'DomainAuthenticated') {
        Write-Info (T "$($p.Name) 为域网络，跳过" "$($p.Name) is a domain network, skipped")
    } elseif ($p.NetworkCategory -ne 'Private') {
        Set-NetConnectionProfile -InterfaceIndex $p.InterfaceIndex -NetworkCategory Private
        Write-Ok "$($p.Name) -> Private"
    } else {
        Write-Ok (T "$($p.Name) 已是 Private" "$($p.Name) is already Private")
    }
}

# ---------- 7. 局域网 IP ----------
Write-Step (T '步骤 7/7：探测局域网 IPv4' 'Step 7/7: Probing LAN IPv4 addresses')
$privateRx = '^(10\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[01])\.)'
$ips = Get-NetIPAddress -AddressFamily IPv4 -PrefixOrigin Dhcp,Manual | Where-Object {
    $_.InterfaceAlias -notlike 'vEthernet*' -and
    $_.InterfaceAlias -notlike 'Loopback*'   -and
    $_.InterfaceAlias -notlike 'WSL*'        -and
    $_.IPAddress -match $privateRx
} | Select-Object -ExpandProperty IPAddress -Unique

if (-not $ips) {
    Write-Info (T '未自动识别出局域网 IPv4，请手动执行 ipconfig 查看（通常为 192.168.x.x）' 'No LAN IPv4 detected; run ipconfig manually (usually 192.168.x.x)')
} else {
    foreach ($ip in $ips) { Write-Ok (T "客户端访问地址：http://${ip}:$Port" "Client access URL: http://${ip}:$Port") }
    if ($ips.Count -gt 1) { Write-Info (T '多个地址时，选与客户端同一网段的那个（通常 192.168.x.x）' 'With multiple addresses, pick the one on the same subnet as the client (usually 192.168.x.x)') }
}

# ---------- 汇总 ----------
Write-Host ''
Write-Host (T '==================== 服务端安装完成 ====================' '==================== Server installation done ====================') -ForegroundColor Magenta
Write-Host (T '  启动服务：cloudcli          （默认监听 3001，窗口保持开启）' '  Start server: cloudcli      (listens on 3001 by default; keep the window open)')
Write-Host "  $(T '本机访问' 'Local URL'): http://localhost:$Port"
Write-Host (T '  手动收尾（脚本覆盖不到的两步）：' '  Manual follow-ups (two steps the script cannot cover):')
Write-Host (T '    1) 浏览器打开上面的地址 -> 设置 -> 开启需要的工具（默认全禁用）' '    1) Open the URL above -> Settings -> enable the tools you need (all disabled by default)')
Write-Host (T '    2) 确认 CC Switch 当前供应商可用（终端跑一次 claude）' "    2) Confirm the current CC Switch provider works (run 'claude' once in a terminal)")
Write-Host (T '  客户端电脑：powershell -ExecutionPolicy Bypass -File .\install-client.ps1 -Url http://<上面的地址>' '  Client PC: powershell -ExecutionPolicy Bypass -File .\install-client.ps1 -Url http://<URL above>')
Write-Host (T '  手机：同 WiFi 浏览器直接打开同一地址（Chrome 可"添加到主屏幕"）' '  Phone: open the same URL in a browser on the same Wi-Fi (Chrome: "Add to Home screen")')
Write-Host (T '  排障：docs/research/sprint0-cloudcli-lan-deploy.md §6' '  Troubleshooting: docs/research/sprint0-cloudcli-lan-deploy.md §6')
Write-Host '=======================================================' -ForegroundColor Magenta
