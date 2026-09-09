<#
.SYNOPSIS
  HTTPS 栈一键安装（半自动）：下载 Caddy + ddns-go，生成 Caddyfile，配好环境并拉起 ddns-go。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  自动覆盖 docs/research/sprint0-cloudcli-lan-deploy.md §9.3 中可脚本化的部分：
    1. 创建 StackDir（默认 D:\Software\cloudcli-https）与 certs\ 子目录
    2. 从 GitHub Release 下载 caddy.exe 与 ddns-go.exe（已存在则跳过，-Update 升级；
       GitHub 直连失败可用 -CaddyZip / -DdnsZip 指向手动下载的 zip）
    3. 生成 Caddyfile（已存在则保持不动）：域名:443 TLS 终结 -> 127.0.0.1:3001
    4. 调用同目录 enable-https.ps1：防火墙放行 443 + 网络改专用 + hosts 钉定
    5. 拉起 ddns-go 并打开管理页 http://127.0.0.1:9876

  剩两步手工活（涉及密钥，刻意不自动化，命令见结尾打印）：
    a. ddns-go 管理页填腾讯云密钥 + 域名（勿设 httpinterface，§9.5-⑧）
    b. acme.sh 签发证书（必须 --dns dns_tencent 全名，§9.5-①）

  幂等：重复运行跳过已就位的组件，只补缺。需要管理员权限（防火墙 / hosts）。

  注意：menu.ps1 / setup-autostart.ps1 的 -StackDir 默认值与本脚本一致；换了目录
  要给它们也传同样的 -StackDir，否则菜单与自启仍指向旧目录。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\install-https.ps1
  powershell -ExecutionPolicy Bypass -File .\install-https.ps1 -Update       # 升级两个 exe
  powershell -ExecutionPolicy Bypass -File .\install-https.ps1 -Domain ai.jackqi.cn -Port 3001
  powershell -ExecutionPolicy Bypass -File .\install-https.ps1 -CaddyZip C:\Users\me\Downloads\caddy_2.11.4_windows_amd64.zip
#>
[CmdletBinding()]
param(
    [string]$StackDir = 'D:\Software\cloudcli-https',
    [string]$Domain = 'ai.jackqi.cn',
    [int]$Port = 3001,
    [string]$CaddyZip = '',
    [string]$DdnsZip = '',
    [switch]$Update,
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

$ErrorActionPreference = 'Stop'

# ---------- 语言 / 输出辅助 ----------
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }

function Write-Step { param([string]$Message) Write-Host "`n==> $Message" -ForegroundColor Cyan }
function Write-Ok   { param([string]$Message) Write-Host "    [OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "    [i ] $Message"  -ForegroundColor DarkGray }
function Write-Warn { param([string]$Message) Write-Host "    [!] $Message"   -ForegroundColor Yellow }
function Write-Bad  { param([string]$Message) Write-Host "    [X ] $Message"  -ForegroundColor Red }

# PS 5.1 默认不启用 TLS 1.2，GitHub API/下载会握手失败；关进度条可大幅加速 Invoke-WebRequest
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'

# GitHub API 必须带 User-Agent，否则 403
$ghHeaders = @{ 'User-Agent' = 'sprint0-install-https' }

# ---------- 0. 管理员检查 ----------
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Bad (T '请以管理员身份运行：先打开管理员 PowerShell，再执行' 'Please run as administrator: open an elevated PowerShell first, then run')
    Write-Host  '    powershell -ExecutionPolicy Bypass -File .\install-https.ps1' -ForegroundColor Yellow
    exit 1
}

# ---------- 组件下载/落位（GitHub Release，本地 zip 兜底）----------
function Install-StackComponent {
    param(
        [string]$Label,          # 展示名（Caddy / ddns-go）
        [string]$Repo,           # GitHub owner/repo
        [string]$AssetPattern,   # release 资产名正则（zip）
        [string]$ExeName,        # 落位后的文件名
        [string]$ZipArg,         # 本地 zip 兜底参数名（-CaddyZip / -DdnsZip）
        [string]$LocalZip,       # 本地 zip 路径（空则在线下载）
        [string]$ReleasesPage    # 失败时指给用户的手动下载页
    )
    $dest = Join-Path $StackDir $ExeName
    if ((Test-Path $dest) -and -not $Update) {
        Write-Ok (T "$Label 已存在，跳过（升级：加 -Update 重跑）" "$Label already present, skipped (to upgrade: re-run with -Update)")
        return $true
    }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ('sprint0-' + [GUID]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        $zip = ''
        if ($LocalZip) {
            if (-not (Test-Path $LocalZip)) { throw (T "找不到 $LocalZip" "Not found: $LocalZip") }
            $zip = $LocalZip
        } else {
            Write-Info (T "查询 $Repo 最新版本 ..." "Querying latest release of $Repo ...")
            $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" `
                -Headers $ghHeaders -TimeoutSec 30 -UseBasicParsing
            $asset = $rel.assets | Where-Object { $_.name -match $AssetPattern } | Select-Object -First 1
            if (-not $asset) { throw "no release asset matches '$AssetPattern'" }
            $zip = Join-Path $tmp $asset.name
            Write-Info (T "下载 $($asset.name)（$([math]::Round($asset.size / 1MB, 1)) MB）..." "Downloading $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) MB) ...")
            Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zip `
                -Headers $ghHeaders -TimeoutSec 600 -UseBasicParsing
        }
        $extract = Join-Path $tmp 'x'
        Expand-Archive -Path $zip -DestinationPath $extract -Force
        $found = Get-ChildItem -Path $extract -Recurse -Filter $ExeName | Select-Object -First 1
        if (-not $found) { throw "no $ExeName inside the archive" }
        Copy-Item -Path $found.FullName -Destination $dest -Force
        Write-Ok (T "$Label -> $dest（$([math]::Round((Get-Item $dest).Length / 1MB, 1)) MB）" "$Label -> $dest ($([math]::Round((Get-Item $dest).Length / 1MB, 1)) MB)")
        return $true
    } catch {
        Write-Bad (T "$Label 安装失败：$($_.Exception.Message)" "$Label install failed: $($_.Exception.Message)")
        Write-Info (T "GitHub 直连不稳时：浏览器手动下载 zip，重跑本脚本并加 $ZipArg <zip 路径>" "If GitHub is unreachable: download the zip in a browser, then re-run with $ZipArg <zip path>")
        Write-Info (T "    手动下载页：$ReleasesPage" "    Manual download page: $ReleasesPage")
        return $false
    } finally {
        Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ---------- 1. 目录 ----------
Write-Step (T '步骤 1/5：创建栈目录' 'Step 1/5: Creating stack directories')
New-Item -ItemType Directory -Path $StackDir -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $StackDir 'certs') -Force | Out-Null
Write-Ok (T "栈目录就绪：$StackDir（含 certs\，证书将放这里）" "Stack directory ready: $StackDir (certs\ included; certificates go there)")
Write-Info (T '此目录今后含密钥与证书私钥，勿分发（§9.1）' 'It will hold secrets and private keys from now on - do not distribute (see deploy doc section 9.1)')

# ---------- 2. 下载 caddy.exe / ddns-go.exe ----------
Write-Step (T '步骤 2/5：安装 Caddy 与 ddns-go（GitHub Release，幂等）' 'Step 2/5: Installing Caddy and ddns-go (GitHub releases, idempotent)')
$caddyOk = Install-StackComponent -Label 'Caddy'   -Repo 'caddyserver/caddy' -AssetPattern 'windows_amd64\.zip$' `
    -ExeName 'caddy.exe'   -ZipArg '-CaddyZip' -LocalZip $CaddyZip `
    -ReleasesPage 'https://github.com/caddyserver/caddy/releases/latest'
$ddnsOk  = Install-StackComponent -Label 'ddns-go' -Repo 'jeessy2/ddns-go'   -AssetPattern 'windows_(x86_64|x64)\.zip$' `
    -ExeName 'ddns-go.exe' -ZipArg '-DdnsZip'  -LocalZip $DdnsZip `
    -ReleasesPage 'https://github.com/jeessy2/ddns-go/releases/latest'

# ---------- 3. Caddyfile ----------
Write-Step (T '步骤 3/5：生成 Caddyfile（已存在则保持不动）' 'Step 3/5: Generating Caddyfile (left untouched if present)')
$caddyfile = Join-Path $StackDir 'Caddyfile'
$certCer   = Join-Path $StackDir ('certs\' + $Domain + '.fullchain.cer')
$certKey   = Join-Path $StackDir ('certs\' + $Domain + '.key')
if (Test-Path $caddyfile) {
    Write-Ok (T "Caddyfile 已存在：$caddyfile（如需改域名/端口请手动编辑）" "Caddyfile already exists: $caddyfile (edit it manually to change domain/port)")
} else {
    # 变量后紧跟冒号必须写 ${Domain}（§9.5-④ 作用域语法坑）
    $lines = @(
        '{',
        '    auto_https disable_redirects',
        '}',
        '',
        "${Domain}:443 {",
        "    tls `"$certCer`" `"$certKey`"",
        '    reverse_proxy 127.0.0.1:' + $Port,
        '}'
    )
    Set-Content -Path $caddyfile -Value $lines -Encoding ascii
    Write-Ok (T "已生成：$caddyfile（${Domain}:443 -> 127.0.0.1:${Port}）" "Generated: $caddyfile (${Domain}:443 -> 127.0.0.1:${Port})")
}

# ---------- 4. HTTPS 环境（防火墙 / 专用网络 / hosts）----------
Write-Step (T '步骤 4/5：HTTPS 环境配置（复用 enable-https.ps1，幂等）' 'Step 4/5: HTTPS environment setup (via enable-https.ps1, idempotent)')
$enableHttps = Join-Path $PSScriptRoot 'enable-https.ps1'
if (Test-Path $enableHttps) {
    powershell -NoProfile -ExecutionPolicy Bypass -File $enableHttps -Lang $script:lang
} else {
    Write-Warn (T "缺少同目录的 enable-https.ps1，跳过环境配置（单独跑 bin\enable-https.bat 补上）" "enable-https.ps1 missing next to this script, skipped (run bin\enable-https.bat separately)")
}

# ---------- 5. 拉起 ddns-go 并打开管理页 ----------
Write-Step (T '步骤 5/5：启动 ddns-go 并打开管理页' 'Step 5/5: Starting ddns-go and opening its admin page')
if ($ddnsOk -and -not (Get-NetTCPConnection -LocalPort 9876 -State Listen -ErrorAction SilentlyContinue)) {
    Start-Process -FilePath (Join-Path $StackDir 'ddns-go.exe') -ArgumentList `
        '-c', (Join-Path $StackDir 'ddns-go.yaml'), '-l', ':9876', '-f', '300' -WindowStyle Hidden
    Start-Sleep -Seconds 2
}
if (Get-NetTCPConnection -LocalPort 9876 -State Listen -ErrorAction SilentlyContinue) {
    Start-Process 'http://127.0.0.1:9876'
    Write-Ok (T 'ddns-go 已在运行，管理页已在浏览器打开：http://127.0.0.1:9876' 'ddns-go is running; admin page opened in browser: http://127.0.0.1:9876')
} else {
    Write-Info (T 'ddns-go 未运行（可能下载失败或启动中）；也可稍后用总控菜单选项 1/9 拉起' 'ddns-go not running (download may have failed, or it is still starting); menu options 1/9 can start it later')
}

# ---------- 汇总 ----------
$certReady = (Test-Path $certCer) -and (Test-Path $certKey)
Write-Host ''
Write-Host (T '==================== HTTPS 栈安装完成 ====================' '==================== HTTPS stack installed ====================') -ForegroundColor Magenta
if ($caddyOk -and $ddnsOk) {
    Write-Host (T '  已就位：caddy.exe / ddns-go.exe / Caddyfile / 防火墙 443 / hosts 钉定' '  In place: caddy.exe / ddns-go.exe / Caddyfile / firewall 443 / hosts pinning')
} else {
    Write-Host (T '  有组件未就位（见上方 [X ] 行），按提示补齐后重跑本脚本' '  Some components missing (see the [X ] lines above); fix and re-run this script')
}
Write-Host ''
if ($certReady) {
    Write-Host (T '  证书已存在，无需重签；直接启动整栈：总控菜单选项 1，或双击 autostart-on.bat' '  Certificates already present, no need to re-issue; start the stack via menu option 1 or autostart-on.bat')
} else {
    Write-Host (T '  剩两步手工活（涉及密钥，刻意不自动化）：' 'Two manual steps remain (they involve secrets, deliberately not automated):')
    Write-Host (T '  1) ddns-go 管理页（浏览器已打开）：服务商=腾讯云、填 SecretId/SecretKey' '  1) ddns-go admin page (opened in browser): provider = Tencent Cloud, enter SecretId/SecretKey')
    Write-Host (T "     （来自项目根 .env）、IPv4 来源选网卡 WLAN、域名 $Domain，保存即建/更新 A 记录" "     (from the project .env), IPv4 from NIC WLAN, domain $Domain; saving creates/updates the A record")
    Write-Host (T '     [!] 不要设置"HTTP 绑定网卡"（httpinterface）相关选项 —— 部署文档 §9.5-⑧' '     [!] Do NOT set the "HTTP bind NIC" (httpinterface) option - deploy doc section 9.5-8')
    Write-Host ''
    Write-Host (T '  2) Git Bash 里签发证书（先 export Tencent_SecretId / Tencent_SecretKey）：' '  2) Issue the certificate in Git Bash (export Tencent_SecretId / Tencent_SecretKey first):')
    Write-Host     "     acme.sh --issue --dns dns_tencent -d $Domain --server letsencrypt" -ForegroundColor Yellow
    Write-Host     ('     acme.sh --install-cert -d ' + $Domain + ' --ecc --fullchain-file ' + $certCer + `
                    ' --key-file ' + $certKey + ' --reloadcmd "' + (Join-Path $StackDir 'caddy.exe') + ' reload --config ' + $caddyfile + '"') -ForegroundColor Yellow
    Write-Host (T '     [!] 必须传全名 --dns dns_tencent（不是 tencent / dnspod）—— 部署文档 §9.5-①' '     [!] The full name --dns dns_tencent is required (not tencent / dnspod) - deploy doc section 9.5-1')
    Write-Host (T '     [!] 此时 Caddy 尚未运行，install-cert 的 reloadcmd 报错可忽略（证书已落位）' '     [!] Caddy is not running yet; an error from install-cert reloadcmd can be ignored (the cert is in place)')
    Write-Host ''
    Write-Host (T '  然后启动：总控菜单选项 1（start-here.bat），或双击 bin\autostart-on.bat 注册开机自启' '  Then start the stack: menu option 1 (start-here.bat), or double-click bin\autostart-on.bat for autostart')
}
Write-Host ''
Write-Host (T "  验证：curl https://$Domain 返回 200；手机同 WiFi 打开应见可信锁标" "  Verify: curl https://$Domain returns 200; the phone on the same Wi-Fi should show a trusted padlock")
Write-Host (T '  排障：部署文档 §6 / §9.5 踩坑实录' '  Troubleshooting: deploy doc sections 6 / 9.5')
Write-Host (T '  注意：menu.ps1 与 setup-autostart.ps1 默认 -StackDir 与本脚本一致；' '  Note: menu.ps1 and setup-autostart.ps1 default to the same -StackDir as this script;')
Write-Host (T '        换了目录要给它们也传同样的 -StackDir' '        if you changed it, pass the same -StackDir to them as well')
Write-Host '=========================================================' -ForegroundColor Magenta

if (-not ($caddyOk -and $ddnsOk)) { exit 1 }
