<#
.SYNOPSIS
  HTTPS 栈一键安装（半自动）：下载带腾讯云 DNS 插件的 Caddy，生成插件式 Caddyfile 并配好环境。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  自动覆盖 docs/research/sprint0-cloudcli-lan-deploy.md §9.3 中可脚本化的部分：
    1. 创建 StackDir（默认 D:\Software\cloudcli-https）与 certs\ 子目录，并把目录权限收紧
       为「运行账户 + SYSTEM + Administrators」（断开 D:\ 的默认继承——该继承把
       Authenticated Users 授为可修改，本机任何登录用户都能读写 .env 与 caddy.exe）
    2. 下载 caddy.exe（caddyserver.com 按需构建，内置 tencentcloud DNS 插件，ADR-0003；
       已存在则跳过，-Update 升级；失败可 -CaddyZip 指向手动下载的文件——
       接受构建站下载的裸 .exe 或自行压缩的 zip）
    3. 生成插件式 Caddyfile（已存在则保持不动）：域名:443 TLS 终结 -> 127.0.0.1:3001，
       证书经 DNS-01 自动签发/续期（凭证以 {env.*} 引用不落明文，ADR-0003）
    4. 调用同目录 enable-https.ps1：防火墙放行 443 + 网络改专用 + hosts 钉定

  剩一步配置活（涉及密钥，走独立脚本 / 装机向导，命令见结尾打印）：
    a. set-tencent-key.ps1：腾讯云 SecretId/Key 写入栈目录 .env
  之后启动 Caddy 即自动签发证书（DNS-01 免 80/443 入站，首次约 1~2 分钟）。
  注意：{env.*} 从 caddy 进程环境读取——工作台托管 / 自启链 / 总控菜单会自动注入；
  裸跑 caddy.exe 前需自行 set TENCENT_SECRET_ID / TENCENT_SECRET_KEY。

  幂等：重复运行跳过已就位的组件，只补缺。需要管理员权限（防火墙 / hosts）。

  注意：menu.ps1 / setup-autostart.ps1 的 -StackDir 默认值与本脚本一致；换了目录
  要给它们也传同样的 -StackDir，否则菜单与自启仍指向旧目录。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\install-https.ps1
  powershell -ExecutionPolicy Bypass -File .\install-https.ps1 -Update       # 升级 caddy.exe
  powershell -ExecutionPolicy Bypass -File .\install-https.ps1 -Domain ai.jackqi.cn -Port 3001
  powershell -ExecutionPolicy Bypass -File .\install-https.ps1 -CaddyZip C:\Users\me\Downloads\caddy_with_tencentcloud.exe
#>
[CmdletBinding()]
param(
    [string]$StackDir = 'D:\Software\cloudcli-https',
    [string]$Domain = 'ai.jackqi.cn',
    [int]$Port = 3001,
    [string]$CaddyZip = '',
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

# PS 5.1 默认不启用 TLS 1.2，构建站下载会握手失败；关进度条可大幅加速 Invoke-WebRequest
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'

# Caddy 插件构建（ADR-0003）：caddyserver.com 按需编译，插件版本锁定；
# 升级 = 显式改这里（构建参数随 manifest 登记，见 specs/006）
$CaddyPluginModule = 'github.com/caddy-dns/tencentcloud@v0.4.3'
$CaddyBuildUrl = "https://caddyserver.com/api/download?os=windows&arch=amd64&p=$([uri]::EscapeDataString($CaddyPluginModule))"
$CaddyManualPage = 'https://caddyserver.com/download'

# ---------- 0. 管理员检查 ----------
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Bad (T '请以管理员身份运行：先打开管理员 PowerShell，再执行' 'Please run as administrator: open an elevated PowerShell first, then run')
    Write-Host  '    powershell -ExecutionPolicy Bypass -File .\install-https.ps1' -ForegroundColor Yellow
    exit 1
}

# ---------- Caddy 插件构建下载（caddyserver.com 按需编译，ADR-0003）----------
function Install-CaddyPluginBuild {
    param(
        [string]$ExeName,          # 落位文件名（caddy.exe）
        [string]$LocalPkg,         # 手动兜底：本地 zip 或构建站下载的裸 exe
        [switch]$UpdateSwitch
    )
    $dest = Join-Path $StackDir $ExeName
    if ((Test-Path $dest) -and -not $UpdateSwitch) {
        Write-Ok (T "Caddy 已存在，跳过（升级：加 -Update 重跑；DNS 插件随构建内置）" "Caddy already present, skipped (to upgrade: re-run with -Update; the DNS plugin ships inside this build)")
        return $true
    }
    $tmp = Join-Path ([IO.Path]::GetTempPath()) ('sprint0-' + [GUID]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        $downloaded = ''
        if ($LocalPkg) {
            if (-not (Test-Path $LocalPkg)) { throw (T "找不到 $LocalPkg" "Not found: $LocalPkg") }
            if ([IO.Path]::GetExtension($LocalPkg) -ieq '.exe') {
                $downloaded = $LocalPkg            # 构建站下载的裸 exe 直接落位
            } else {
                $extract = Join-Path $tmp 'x'
                Expand-Archive -Path $LocalPkg -DestinationPath $extract -Force
                $found = Get-ChildItem -Path $extract -Recurse -Filter $ExeName | Select-Object -First 1
                if (-not $found) { throw "no $ExeName inside the archive" }
                $downloaded = $found.FullName
            }
        } else {
            Write-Info (T "请求按需构建（$CaddyPluginModule，云端编译需数分钟、下载较慢，请耐心等待）..." "Requesting on-demand build ($CaddyPluginModule; cloud compilation takes minutes and the download is slow, please wait)...")
            $downloaded = Join-Path $tmp 'caddy-build.exe'
            Invoke-WebRequest -Uri $CaddyBuildUrl -OutFile $downloaded -TimeoutSec 1800 -UseBasicParsing
        }
        # 完整性粗检：必须是 Windows PE（MZ 头）且体积合理，防止把错误页当 exe 落位
        if ((Get-Item $downloaded).Length -lt 10MB) { throw 'downloaded file is too small to be caddy.exe' }
        $fs = [IO.File]::OpenRead($downloaded)
        try {
            $head = New-Object byte[] 2
            $null = $fs.Read($head, 0, 2)
            if ([Text.Encoding]::ASCII.GetString($head) -ne 'MZ') { throw 'downloaded file is not a Windows executable' }
        } finally { $fs.Close() }
        Copy-Item -Path $downloaded -Destination $dest -Force
        Write-Ok (T "Caddy（tencentcloud 插件版）-> $dest（$([math]::Round((Get-Item $dest).Length / 1MB, 1)) MB）" "Caddy (tencentcloud plugin build) -> $dest ($([math]::Round((Get-Item $dest).Length / 1MB, 1)) MB)")
        return $true
    } catch {
        Write-Bad (T "Caddy 安装失败：$($_.Exception.Message)" "Caddy install failed: $($_.Exception.Message)")
        Write-Info (T "构建站不可达时：浏览器打开下载页（Windows amd64 + 插件 $CaddyPluginModule），把得到的 exe 按下一步提示重跑本脚本加 -CaddyZip <文件路径>" "If the build service is unreachable: open the download page in a browser (Windows amd64 + plugin $CaddyPluginModule), then re-run with -CaddyZip <path-to-exe>")
        Write-Info (T "    手动下载页：$CaddyManualPage" "    Manual download page: $CaddyManualPage")
        return $false
    } finally {
        Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ---------- 权限收紧辅助（栈目录含密钥与证书私钥） ----------
# 背景：D:\ 的默认继承把 Authenticated Users 授为「可修改」——本机任何登录用户既能
# 读走 .env 里的腾讯云 SecretId/Key，也能覆写 caddy.exe（后者等于
# 以托管账户执行任意代码，危害更甚）。故建目录后立即断开继承、只授三方。
# 运行时账户取「当前身份」∪「交互登录用户」：本脚本需管理员提权，正常提权不换 SID，
# 但若以另一个管理员账户提权，只授当前身份会把真正的运行账户（自启任务用 Interactive
# 登录用户）挡在门外，进而打断 Caddy 读证书 / 写日志。
# 失败不中断安装（栈仍可用），但如实报红——安全属性未达成必须显式可见。
function Protect-StackDir {
    param([Parameter(Mandatory)][string]$Path)

    $Path = $Path.TrimEnd('\')
    $sids = [ordered]@{}
    $sids[[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value] = $true
    $interactive = ''
    try { $interactive = (Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).UserName } catch { }
    if ($interactive) {
        try {
            $sids[(New-Object System.Security.Principal.NTAccount($interactive)).Translate(
                [System.Security.Principal.SecurityIdentifier]).Value] = $true
        } catch {
            Write-Warn (T "无法解析交互登录用户 $interactive，跳过该项授权" "Could not resolve interactive user $interactive; skipping that grant")
        }
    }
    $sids['S-1-5-18']     = $true  # NT AUTHORITY\SYSTEM
    $sids['S-1-5-32-544'] = $true  # BUILTIN\Administrators

    # 用 SID 而非账户名：中文账户名经 icacls 命令行会遇编码 / 本地化问题
    $grants = @($sids.Keys | ForEach-Object { "*${_}:(OI)(CI)(F)" })
    & icacls.exe $Path /inheritance:r /grant:r $grants | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Bad (T "权限收紧失败（icacls 退出码 $LASTEXITCODE）：$Path 仍为继承的宽松权限，.env 与 caddy.exe 对本机其他用户可读写" "Failed to tighten ACL (icacls exit $LASTEXITCODE): $Path keeps its inherited permissive ACL - .env and caddy.exe stay readable/writable by other local users")
        return $false
    }
    Write-Ok (T "权限已收紧：$Path（仅运行账户 + SYSTEM + Administrators）" "ACL tightened: $Path (runtime account + SYSTEM + Administrators only)")
    return $true
}

# ---------- 1. 目录 ----------
Write-Step (T '步骤 1/4：创建栈目录' 'Step 1/4: Creating stack directories')
New-Item -ItemType Directory -Path $StackDir -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $StackDir 'certs') -Force | Out-Null
Write-Ok (T "栈目录就绪：$StackDir（含 certs\，证书将放这里）" "Stack directory ready: $StackDir (certs\ included; certificates go there)")
Write-Info (T '此目录今后含密钥与证书私钥，勿分发（§9.1）' 'It will hold secrets and private keys from now on - do not distribute (see deploy doc section 9.1)')
Protect-StackDir -Path $StackDir | Out-Null

# ---------- 2. 下载 caddy.exe（插件构建） ----------
Write-Step (T '步骤 2/4：安装 Caddy（tencentcloud 插件构建，幂等）' 'Step 2/4: Installing Caddy (tencentcloud plugin build, idempotent)')
$caddyOk = Install-CaddyPluginBuild -ExeName 'caddy.exe' -LocalPkg $CaddyZip -UpdateSwitch:$Update

# ---------- 3. Caddyfile ----------
Write-Step (T '步骤 3/4：生成 Caddyfile（已存在则保持不动）' 'Step 3/4: Generating Caddyfile (left untouched if present)')
$caddyfile = Join-Path $StackDir 'Caddyfile'
$certCer   = Join-Path $StackDir ('certs\' + $Domain + '.fullchain.cer')
$certKey   = Join-Path $StackDir ('certs\' + $Domain + '.key')
if (Test-Path $caddyfile) {
    Write-Ok (T "Caddyfile 已存在：$caddyfile（如需改域名/端口请手动编辑）" "Caddyfile already exists: $caddyfile (edit it manually to change domain/port)")
} else {
    # 变量后紧跟冒号必须写 ${Domain}（§9.5-④ 作用域语法坑）
    # 插件式 TLS（ADR-0003）：DNS-01 自动签发/续期；凭证经 {env.*} 引用不落明文，
    # 由工作台托管 / 自启链 / 总控菜单在 spawn 时从栈 .env 注入
    # 注意：tencentcloud 插件必须用块内键值（secret_id/secret_key）写法——
    # 位置参数形式 validate 报 wrong argument count（2026-09-10 实证）
    # 注意：数组字面量元素不可用 'str' + $var 拼接（PS 5.1 会把逗号解析为
    # + 的右操作数，导致值被拆分）——一律用 "${var}" 插值（2026-09-10 实证）
    $lines = @(
        '{',
        '    auto_https disable_redirects',
        '}',
        '',
        "${Domain}:443 {",
        '    tls {',
        '        dns tencentcloud {',
        '            secret_id {env.TENCENT_SECRET_ID}',
        '            secret_key {env.TENCENT_SECRET_KEY}',
        '        }',
        '    }',
        "    reverse_proxy 127.0.0.1:${Port}",
        '}'
    )
    Set-Content -Path $caddyfile -Value $lines -Encoding ascii
    Write-Ok (T "已生成插件式 Caddyfile：$caddyfile（${Domain}:443 -> 127.0.0.1:${Port}，证书自动签发）" "Generated plugin-style Caddyfile: $caddyfile (${Domain}:443 -> 127.0.0.1:${Port}, certificate auto-issued)")
}

# ---------- 4. HTTPS 环境（防火墙 / 专用网络 / hosts）----------
Write-Step (T '步骤 4/4：HTTPS 环境配置（复用 enable-https.ps1，幂等）' 'Step 4/4: HTTPS environment setup (via enable-https.ps1, idempotent)')
$enableHttps = Join-Path $PSScriptRoot 'enable-https.ps1'
if (Test-Path $enableHttps) {
    powershell -NoProfile -ExecutionPolicy Bypass -File $enableHttps -Lang $script:lang
} else {
    Write-Warn (T "缺少同目录的 enable-https.ps1，跳过环境配置（单独跑 bin\enable-https.bat 补上）" "enable-https.ps1 missing next to this script, skipped (run bin\enable-https.bat separately)")
}

# ---------- 汇总 ----------
$legacyCerts = (Test-Path $certCer) -and (Test-Path $certKey)
$isPluginCaddyfile = $false
if (Test-Path $caddyfile) {
    $isPluginCaddyfile = Select-String -Path $caddyfile -Pattern 'dns\s+tencentcloud' -Quiet
}
Write-Host ''
Write-Host (T '==================== HTTPS 栈安装完成 ====================' '==================== HTTPS stack installed ====================') -ForegroundColor Magenta
if ($caddyOk) {
    Write-Host (T '  已就位：caddy.exe（tencentcloud 插件版）/ Caddyfile / 防火墙 443 / hosts 钉定' '  In place: caddy.exe (tencentcloud build) / Caddyfile / firewall 443 / hosts pinning')
} else {
    Write-Host (T '  有组件未就位（见上方 [X ] 行），按提示补齐后重跑本脚本' '  Some components missing (see the [X ] lines above); fix and re-run this script')
}
Write-Host ''
if ($legacyCerts -and -not $isPluginCaddyfile) {
    Write-Host (T '  检测到旧版证书链（acme.sh 签发）且既有 Caddyfile 原样保留——现有部署不受影响，无需任何动作（ADR-0003 迁移为可选）' '  Legacy certificate chain (acme.sh) detected and the existing Caddyfile is untouched - your deployment is unaffected, nothing to do (ADR-0003 migration is optional)')
} else {
    Write-Host (T '  新栈剩一步配置（涉及密钥，走独立脚本；工作台装机向导会自动代劳）：' '  One configuration step remains for the new stack (it involves secrets; the workbench install wizard automates it):')
    Write-Host (T '  1) 写入腾讯云密钥（SecretId/SecretKey，不回显输入）：' '  1) Store the Tencent Cloud key (SecretId/SecretKey, hidden input):')
    Write-Host     '     powershell -ExecutionPolicy Bypass -File .\set-tencent-key.ps1' -ForegroundColor Yellow
    Write-Host ''
    Write-Host (T '  然后启动：总控菜单选项 1（start-here.bat）/ 工作台装机向导 / 双击 bin\autostart-on.bat。' '  Then start: menu option 1 (start-here.bat) / the workbench install wizard / double-click bin\autostart-on.bat.')
    Write-Host (T '  Caddy 首次启动自动签发证书（DNS-01，免 80/443 入站，约 1~2 分钟），此后自动续期' '  On first start Caddy issues the certificate automatically (DNS-01, no inbound 80/443 needed, about 1-2 minutes), then renews itself')
    Write-Host (T '  [!] 密钥经 {env.*} 引用不落明文：工作台/菜单/自启链启动时会自动注入环境；如需裸跑 caddy.exe，请先自行 set TENCENT_SECRET_ID / TENCENT_SECRET_KEY' '  [!] Keys are referenced via {env.*} (never plaintext): the workbench/menu/autostart chain injects them on start; for a bare caddy.exe run, set TENCENT_SECRET_ID / TENCENT_SECRET_KEY yourself first')
}
Write-Host ''
Write-Host (T "  验证：curl https://$Domain 返回 200；手机同 WiFi 打开应见可信锁标" "  Verify: curl https://$Domain returns 200; the phone on the same Wi-Fi should show a trusted padlock")
Write-Host (T '  排障：部署文档 §6 / §9.5 踩坑实录' '  Troubleshooting: deploy doc sections 6 / 9.5')
Write-Host (T '  注意：menu.ps1 与 setup-autostart.ps1 默认 -StackDir 与本脚本一致；' '  Note: menu.ps1 and setup-autostart.ps1 default to the same -StackDir as this script;')
Write-Host (T '        换了目录要给它们也传同样的 -StackDir' '        if you changed it, pass the same -StackDir to them as well')
Write-Host '=========================================================' -ForegroundColor Magenta

if (-not $caddyOk) { exit 1 }
