<#
.SYNOPSIS
  ddns-go 配置生成 + 拉起：从栈目录 .env 读腾讯云密钥 → 生成 ddns-go.yaml → 启动 ddns-go。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  直连通道的解析配置入口（spec 006）：替代「进 ddns-go 管理页手工填写」的旧流程。
    1. 前置检查：栈目录 / .env（TENCENT_SECRET_ID/KEY，由 set-tencent-key.ps1 写入）
    2. 生成 ddns-go.yaml（已存在则保持不动，幂等——删除后重跑可重新生成）：
       - 服务商 tencentcloud，凭证取自 .env（写入配置文件与 ddns-go 自身行为一致，
         该文件与 .env 同目录同权限，勿分发）
       - IPv4 gettype=url（公网接口法；netInterface 有写内网 IP 前科，004 spec §1）
       - 域名 = -Domain（默认 ai.jackqi.cn）；IPv6 关闭；httpinterface 恒为空（§9.5-⑧）
    3. 拉起 ddns-go（9876 已监听则跳过）并等待首轮解析生效
  注意：无需管理员权限。解析是否写对由工作台向导/看板校验（权威 DNS 比对）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\config-ddnsgo.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File .\config-ddnsgo.ps1 -Domain ai.jackqi.cn
#>
[CmdletBinding()]
param(
    [string]$StackDir = 'D:\Software\cloudcli-https',
    [string]$Domain = 'ai.jackqi.cn',
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

$ErrorActionPreference = 'Stop'

# ---------- 语言 / 输出辅助（sprint0 约定：T() + Write-Ok/Info/Bad） ----------
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }
function Write-Ok   { param([string]$Message) Write-Host "[OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "[i ] $Message"  -ForegroundColor DarkGray }
function Write-Warn { param([string]$Message) Write-Host "[!] $Message"   -ForegroundColor Yellow }
function Write-Bad  { param([string]$Message) Write-Host "[X ] $Message"  -ForegroundColor Red }

# ---------- 前置检查 ----------
$EnvFile = Join-Path $StackDir '.env'
if (-not (Test-Path $StackDir)) {
    Write-Bad (T "未找到栈目录：$StackDir" "Stack directory not found: $StackDir")
    Write-Bad (T '请先运行 install-https.ps1 安装 HTTPS 栈。' 'Run install-https.ps1 first to install the HTTPS stack.')
    exit 1
}
if (-not (Test-Path $EnvFile)) {
    Write-Bad (T "未找到凭证文件：$EnvFile" "Credential file not found: $EnvFile")
    Write-Bad (T '请先运行 set-tencent-key.ps1 写入腾讯云密钥。' 'Run set-tencent-key.ps1 first to store the Tencent Cloud key.')
    exit 1
}

# 从 .env 提取凭证（不回显、不写入任何输出）
$id = ''
$key = ''
foreach ($line in [System.IO.File]::ReadAllLines($EnvFile)) {
    if ($line -match '^\s*TENCENT_SECRET_ID\s*=(.*)$')  { $id  = $Matches[1].Trim() }
    if ($line -match '^\s*TENCENT_SECRET_KEY\s*=(.*)$') { $key = $Matches[1].Trim() }
}
if ([string]::IsNullOrWhiteSpace($id) -or [string]::IsNullOrWhiteSpace($key)) {
    Write-Bad (T '.env 中缺少 TENCENT_SECRET_ID / TENCENT_SECRET_KEY。' 'TENCENT_SECRET_ID / TENCENT_SECRET_KEY missing from .env.')
    Write-Bad (T '请先运行 set-tencent-key.ps1 写入腾讯云密钥。' 'Run set-tencent-key.ps1 first to store the Tencent Cloud key.')
    exit 1
}

# ---------- 生成 ddns-go.yaml（已存在则保持不动，幂等） ----------
$yamlPath = Join-Path $StackDir 'ddns-go.yaml'
if (Test-Path $yamlPath) {
    Write-Ok (T "ddns-go.yaml 已存在，保持不动（如需重新生成请先删除：$yamlPath）" "ddns-go.yaml already exists, left untouched (delete it first to regenerate: $yamlPath)")
} else {
    # 结构与 ddns-go 自身写出的格式一致（生产实例实测样本，2026-09-10）
    # 凭证写进本文件与 ddns-go 常规行为一致；该文件含密钥，勿分发
    $yaml = @"
dnsconf:
    - name: ""
      ipv4:
        enable: true
        gettype: url
        url: https://ddns.oray.com/checkip, https://ip.3322.net, https://4.ipw.cn, https://v4.yinghualuo.cn/bejson, https://myip.ipip.net
        netinterface: ""
        cmd: ""
        domains:
            - $Domain
      ipv6:
        enable: false
        gettype: netInterface
        url: https://speed.neu6.edu.cn/getIP.php, https://v6.ident.me, https://6.ipw.cn, https://v6.yinghualuo.cn/bejson
        netinterface: ""
        cmd: ""
        ipv6reg: ""
        domains:
            - ""
      dns:
        name: tencentcloud
        id: $id
        secret: $key
        extparam: ""
      ttl: ""
      httpinterface: ""
user:
    username: ""
    password: ""
webhook:
    webhookurl: ""
    webhookrequestbody: ""
    webhookheaders: ""
notallowwanaccess: true
lang: $script:lang
"@
    # UTF-8 无 BOM（与 ddns-go 自身写出的 yaml 一致）
    [System.IO.File]::WriteAllText($yamlPath, $yaml, [System.Text.UTF8Encoding]::new($false))
    Write-Ok (T "已生成：$yamlPath（tencentcloud / url 模式取 IP / 域名 $Domain / IPv6 关闭）" "Generated: $yamlPath (tencentcloud / url-mode IP lookup / domain $Domain / IPv6 off)")
    Write-Info (T '此文件含密钥，勿分发（§9.1）；httpinterface 恒为空（§9.5-⑧）' 'This file holds secrets - do not distribute (deploy doc 9.1); httpinterface stays empty (deploy doc 9.5-8)')
}

# ---------- 拉起 ddns-go（9876 已监听则跳过） ----------
$ddnsExe = Join-Path $StackDir 'ddns-go.exe'
if (-not (Test-Path $ddnsExe)) {
    Write-Bad (T "未找到 ddns-go.exe：$ddnsExe（请先运行 install-https.ps1）" "ddns-go.exe not found: $ddnsExe (run install-https.ps1 first)")
    exit 1
}
if (Get-NetTCPConnection -LocalPort 9876 -State Listen -ErrorAction SilentlyContinue) {
    Write-Ok (T 'ddns-go 已在运行，跳过启动。' 'ddns-go is already running; start skipped.')
} else {
    Start-Process -FilePath $ddnsExe -ArgumentList '-c', $yamlPath, '-l', ':9876', '-f', '300' -WindowStyle Hidden
    Start-Sleep -Seconds 2
    if (Get-NetTCPConnection -LocalPort 9876 -State Listen -ErrorAction SilentlyContinue) {
        Write-Ok (T 'ddns-go 已启动（管理页 http://127.0.0.1:9876，日常无需打开）。' 'ddns-go started (admin page http://127.0.0.1:9876, normally no need to open it).')
    } else {
        Write-Warn (T 'ddns-go 启动后未监听 9876（可能仍在初始化），可稍后在工作台看板确认。' 'ddns-go did not listen on 9876 right after start (may still be initializing); check the workbench dashboard later.')
    }
}
Write-Info (T '解析是否写对由工作台向导/看板做权威 DNS 比对；本机网络若为 NAT/蜂窝热点，直连可能不可达（向导会如实警示）。' 'Whether the record is correct is verified by the workbench wizard/dashboard (authoritative DNS); on NAT/cellular networks direct access may be unreachable (the wizard warns honestly).')
