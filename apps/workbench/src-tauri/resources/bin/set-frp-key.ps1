<#
.SYNOPSIS
  SakuraFrp 访问密钥写入：交互输入（不回显）→ 栈目录 .env 的 SAKURA_FRP_KEY。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  穿透通道（spec 004）的凭证配置入口。密钥路径：键盘 → 本脚本（不回显）
  → 直接写 D:\Software\cloudcli-https\.env —— 全程不进工作台 APP 内存、
  不走 IPC、不写任何日志（与 reset-ddns-password.ps1 同款安全设计）。
  流程：
    1. 提示密钥获取入口（natfrp.com 用户中心）
    2. 键入密钥（不回显，两次确认，空/不一致拒绝）
    3. 写入 .env：已有 SAKURA_FRP_KEY 行则原位替换，其余行原样保留；
       无 .env 则创建（带说明注释头，UTF-8 BOM 兼容 PowerShell 5.1 回读）
    4. 复核：回读匹配行，仅显示密钥末 4 位（确认写入对象，不显示全值）
  注意：无需管理员权限（栈目录为当前用户可写）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\set-frp-key.ps1
#>
[CmdletBinding()]
param(
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
function Write-Bad  { param([string]$Message) Write-Host "[X ] $Message"  -ForegroundColor Red }

# SecureString → 明文（仅用于写入 .env，不落任何输出/日志）
function ConvertTo-PlainText {
    param([Security.SecureString]$Secure)
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Secure)
    try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

# ---------- 路径常量（与 APP consts.rs 对齐） ----------
$StackDir = 'D:\Software\cloudcli-https'
$EnvFile  = Join-Path $StackDir '.env'
$KeyVar   = 'SAKURA_FRP_KEY'

# ---------- 前置检查 ----------
if (-not (Test-Path $StackDir)) {
    Write-Bad (T "未找到栈目录：$StackDir" "Stack directory not found: $StackDir")
    Write-Bad (T '请先运行 install-server.ps1 安装服务端。' 'Run install-server.ps1 first to install the server.')
    exit 1
}

# ---------- 输入（不回显 + 两次确认） ----------
Write-Info (T '访问密钥获取：https://www.natfrp.com/user/ → 查看访问密钥' 'Get your key: https://www.natfrp.com/user/ → View Access Key')
$sec1 = Read-Host -AsSecureString (T '访问密钥（输入不回显）' 'Access key (input hidden)')
$sec2 = Read-Host -AsSecureString (T '再输一次确认' 'Confirm the key')
$key1 = ConvertTo-PlainText $sec1
$key2 = ConvertTo-PlainText $sec2

if ([string]::IsNullOrWhiteSpace($key1)) {
    Write-Bad (T '密钥为空，未写入。' 'Empty key; nothing written.')
    exit 1
}
if ($key1 -cne $key2) {
    Write-Bad (T '两次输入不一致，未写入。' 'The two entries do not match; nothing written.')
    exit 1
}

# ---------- 写入 .env（替换既有行 / 追加 / 建档） ----------
$newLine = "$KeyVar=$key1"
$utf8Bom = [System.Text.UTF8Encoding]::new($true)
if (Test-Path $EnvFile) {
    $lines = [System.IO.File]::ReadAllLines($EnvFile)
    $found = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match "^\s*$KeyVar\s*=") {
            $lines[$i] = $newLine
            $found = $true
        }
    }
    if (-not $found) { $lines += $newLine }
    [System.IO.File]::WriteAllLines($EnvFile, $lines, $utf8Bom)
    Write-Info (T '已更新既有 .env 中的密钥行（其余行原样保留）。' 'Updated the key line in the existing .env (other lines untouched).')
} else {
    $header = @(
        '# Environment variables (sensitive credentials; never commit, never log).'
        "# $KeyVar : SakuraFrp access key for the tunnel client frpc (spec 004)."
        $newLine
    )
    [System.IO.File]::WriteAllLines($EnvFile, $header, $utf8Bom)
    Write-Info (T '已创建 .env 并写入密钥。' 'Created .env and wrote the key.')
}

# ---------- 复核（仅显示末 4 位） ----------
$check = Select-String -Path $EnvFile -Pattern "^\s*$KeyVar\s*=" | Select-Object -First 1
if ($null -eq $check) {
    Write-Bad (T '写入后复核失败：未找到密钥行。' 'Post-write verification failed: key line not found.')
    exit 1
}
$tail = $key1.Substring([Math]::Max(0, $key1.Length - 4))
Write-Ok (T "密钥已写入：$EnvFile（末 4 位：****$tail）" "Key written to $EnvFile (last 4 chars: ****$tail)")
Write-Info (T '可在工作台「访问通道」中切换到穿透通道。' 'You can now switch to the tunnel channel in the workbench.')
