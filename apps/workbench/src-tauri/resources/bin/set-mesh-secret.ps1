<#
.SYNOPSIS
  EasyTier 组网密钥写入：交互输入（不回显）→ <StackDir>\easytier\network-secret。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  组网通道（spec 007）的凭证配置入口。密钥路径：键盘 → 本脚本（不回显）
  → 直接写 D:\Software\cloudcli-https\easytier\network-secret —— 全程不进
  工作台 APP 内存、不走 IPC、不写任何日志（与 set-frp-key.ps1 同款安全设计）。
  组网密钥由你自定（不是从网站获取的凭证）：成员设备（手机/PC 的 EasyTier
  客户端）加入同一网络时输入相同密钥即可互通。
  流程：
    1. 键入密钥（不回显，两次确认，空/不一致拒绝）
    2. 写入 network-secret：UTF-8 无 BOM、单行无尾换行
    3. 复核：回读比对，仅显示密钥末 4 位（确认写入对象，不显示全值）
  注意：无需管理员权限（栈目录为当前用户可写）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\set-mesh-secret.ps1
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
function Write-Bad  { param([string]$Message) Write-Host "[X ] $Message"  -ForegroundColor Red }

# SecureString → 明文（仅用于写入文件，不落任何输出/日志）
function ConvertTo-PlainText {
    param([Security.SecureString]$Secure)
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Secure)
    try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

# ---------- 路径常量（与 APP mesh.rs 对齐） ----------
$MeshDir   = Join-Path $StackDir 'easytier'
$SecretFile = Join-Path $MeshDir 'network-secret'

# ---------- 输入（不回显 + 两次确认） ----------
Write-Info (T '组网密钥由你自定（成员设备加入同一网络时输入相同密钥即可互通）。' 'The mesh secret is chosen by you (enter the same secret on member devices to join the network).')
$sec1 = Read-Host -AsSecureString (T '组网密钥（输入不回显）' 'Mesh secret (input hidden)')
$sec2 = Read-Host -AsSecureString (T '再输一次确认' 'Confirm the secret')
$key1 = (ConvertTo-PlainText $sec1).Trim()
$key2 = (ConvertTo-PlainText $sec2).Trim()

if ([string]::IsNullOrWhiteSpace($key1)) {
    Write-Bad (T '密钥为空，未写入。' 'Empty secret; nothing written.')
    exit 1
}
if ($key1 -cne $key2) {
    Write-Bad (T '两次输入不一致，未写入。' 'The two entries do not match; nothing written.')
    exit 1
}

# ---------- 写入（目录缺失则建；UTF-8 无 BOM 单行） ----------
New-Item -ItemType Directory -Force -Path $MeshDir | Out-Null
# 无 BOM 的原因：工作台（Rust）按文本读取本文件渲染进 config.toml，UTF-8 BOM
# 会作为隐藏字符混进密钥值（Rust 的 trim 不剥 U+FEFF），与成员设备输入不一致
# → 组网握手失败。与 .env 的 BOM 惯例相反——.env 由 PowerShell 自家回读。
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($SecretFile, $key1, $utf8NoBom)
Write-Info (T '已写入组网密钥文件（UTF-8 无 BOM 单行）。' 'Secret file written (UTF-8 without BOM, single line).')

# ---------- 复核（读回比对；仅显示末 4 位，绝不显示全值） ----------
$check = [System.IO.File]::ReadAllText($SecretFile, $utf8NoBom)
if ($check -cne $key1) {
    Write-Bad (T '写入后复核失败：回读内容与输入不一致。' 'Post-write verification failed: file content mismatch.')
    exit 1
}
$tail = $key1.Substring([Math]::Max(0, $key1.Length - 4))
Write-Ok (T "组网密钥已写入：$SecretFile（末 4 位：****$tail）" "Mesh secret written to $SecretFile (last 4 chars: ****$tail)")
Write-Info (T '换钥即吊销：修改密钥后旧成员将断开，重新加入须输入新密钥。' 'Changing the secret revokes access: old members disconnect until they rejoin with the new secret.')
Write-Info (T '可在工作台「访问通道」中切换到组网通道。' 'You can now switch to the mesh channel in the workbench.')
