<#
.SYNOPSIS
  腾讯云 API 密钥写入：交互输入（SecretKey 不回显）→ 栈目录 .env 的 TENCENT_SECRET_ID/KEY。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  HTTPS 证书签发与组网 A 记录维护共用的凭证配置入口（spec 006/008，ADR-0003）。
  密钥路径：键盘 → 本脚本
  → 直接写 D:\Software\cloudcli-https\.env —— 全程不进工作台 APP 内存、不走 IPC、
  不写任何日志（与 set-mesh-secret.ps1 同款安全设计，宪法 §3）。
  消费方：
    - Caddy（tencentcloud DNS 插件）：DNS-01 签发/续期证书（spawn 时经 {env.*} 注入）
    - 工作台 dns_api：DNS 对齐检测与组网 A 记录同步
  流程：
    1. 提示密钥获取入口（console.cloud.tencent.com/cam/capi 新建密钥；
       建议建子用户仅授 QcloudDNSPodFullAccess 再为其建密钥）
    2. 键入 SecretId（可见输入）与 SecretKey（不回显，两次确认，空/不一致拒绝）
    3. 写入 .env：已有对应行则原位替换，其余行原样保留；无 .env 则创建
       （带说明注释头，UTF-8 BOM 兼容 PowerShell 5.1 回读）
    4. 复核：回读匹配行，仅显示 SecretId 末 4 位（不显示 SecretKey 任何部分）
  注意：无需管理员权限（栈目录为当前用户可写）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\set-tencent-key.ps1
#>
[CmdletBinding()]
param(
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto',
    [string]$StackDir = 'D:\Software\cloudcli-https'
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

# SecureString → 明文（仅用于写入 .env，不落任何输出/日志）
function ConvertTo-PlainText {
    param([Security.SecureString]$Secure)
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Secure)
    try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

# ---------- 路径常量（与 APP consts.rs 对齐） ----------
$EnvFile  = Join-Path $StackDir '.env'
$IdVar    = 'TENCENT_SECRET_ID'
$KeyVar   = 'TENCENT_SECRET_KEY'

# ---------- 前置检查 ----------
if (-not (Test-Path $StackDir)) {
    Write-Bad (T "未找到栈目录：$StackDir" "Stack directory not found: $StackDir")
    Write-Bad (T '请先运行 install-https.ps1 安装 HTTPS 栈。' 'Run install-https.ps1 first to install the HTTPS stack.')
    exit 1
}

# ---------- 输入 ----------
Write-Info (T '密钥获取：https://console.cloud.tencent.com/cam/capi → 新建密钥' 'Get keys: https://console.cloud.tencent.com/cam/capi -> Create Key')
Write-Info (T '建议：建子用户仅授 QcloudDNSPodFullAccess 权限，再为其新建密钥' 'Recommended: create a sub-user granted QcloudDNSPodFullAccess only, then create keys for it')

$id = (Read-Host (T 'SecretId' 'SecretId')).Trim()
if ([string]::IsNullOrWhiteSpace($id)) {
    Write-Bad (T 'SecretId 为空，未写入。' 'Empty SecretId; nothing written.')
    exit 1
}
if (-not $id.StartsWith('AKID')) {
    Write-Warn (T 'SecretId 通常以 AKID 开头，请确认来源是 CAM 控制台。' 'SecretId usually starts with AKID - make sure it comes from the CAM console.')
}

$sec1 = Read-Host -AsSecureString (T 'SecretKey（输入不回显）' 'SecretKey (input hidden)')
$sec2 = Read-Host -AsSecureString (T '再输一次确认' 'Confirm the SecretKey')
$key1 = ConvertTo-PlainText $sec1
$key2 = ConvertTo-PlainText $sec2

if ([string]::IsNullOrWhiteSpace($key1)) {
    Write-Bad (T 'SecretKey 为空，未写入。' 'Empty SecretKey; nothing written.')
    exit 1
}
if ($key1 -cne $key2) {
    Write-Bad (T '两次输入不一致，未写入。' 'The two entries do not match; nothing written.')
    exit 1
}

# ---------- 写入 .env（替换既有行 / 追加 / 建档） ----------
$utf8Bom = [System.Text.UTF8Encoding]::new($true)
if (Test-Path $EnvFile) {
    $lines = [System.IO.File]::ReadAllLines($EnvFile)
    $foundId = $false
    $foundKey = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match "^\s*$IdVar\s*=")  { $lines[$i] = "$IdVar=$id";   $foundId  = $true }
        if ($lines[$i] -match "^\s*$KeyVar\s*=") { $lines[$i] = "$KeyVar=$key1"; $foundKey = $true }
    }
    if (-not $foundId)  { $lines += "$IdVar=$id" }
    if (-not $foundKey) { $lines += "$KeyVar=$key1" }
    [System.IO.File]::WriteAllLines($EnvFile, $lines, $utf8Bom)
    Write-Info (T '已更新既有 .env 中的密钥行（其余行原样保留）。' 'Updated the key lines in the existing .env (other lines untouched).')
} else {
    $header = @(
        '# Environment variables (sensitive credentials; never commit, never log).'
        "# $IdVar / $KeyVar : Tencent Cloud CAM key pair (spec 006 / ADR-0003)."
        '# Consumed by: Caddy DNS-01 (tencentcloud plugin), workbench dns_api (mesh A-record sync).'
        "$IdVar=$id"
        "$KeyVar=$key1"
    )
    [System.IO.File]::WriteAllLines($EnvFile, $header, $utf8Bom)
    Write-Info (T '已创建 .env 并写入密钥。' 'Created .env and wrote the keys.')
}

# ---------- 复核（仅显示 SecretId 末 4 位；SecretKey 不显示任何部分） ----------
$idLine  = Select-String -Path $EnvFile -Pattern "^\s*$IdVar\s*="  | Select-Object -First 1
$keyLine = Select-String -Path $EnvFile -Pattern "^\s*$KeyVar\s*=" | Select-Object -First 1
if ($null -eq $idLine -or $null -eq $keyLine) {
    Write-Bad (T '写入后复核失败：未找到密钥行。' 'Post-write verification failed: key lines not found.')
    exit 1
}
$idTail = $id.Substring([Math]::Max(0, $id.Length - 4))
Write-Ok (T "密钥已写入：$EnvFile（SecretId 末 4 位：****$idTail）" "Keys written to $EnvFile (SecretId last 4 chars: ****$idTail)")
Write-Info (T '下一步：Caddy 启动后将自动签发证书；组网装机时用工作台「同步 DNS」建立 A 记录。' 'Next: Caddy will issue the certificate automatically once started; during mesh setup, use "Sync DNS" in the workbench to create the A record.')
