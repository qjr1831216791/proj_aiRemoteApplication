<#
.SYNOPSIS
  SakuraFrp 访问密钥清除：从栈目录 .env 移除 SAKURA_FRP_KEY 行。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  停用穿透通道（spec 007 AC5）后的残留清理：SAKURA_FRP_KEY 只服务于 frpc
  穿透客户端，通道停用后保留即多余暴露面。本脚本从 <StackDir>\.env 移除
  该行，其余行（腾讯云密钥等）原样保留；UTF-8 BOM 形态与 set-frp-key.ps1
  写入惯例一致（PowerShell 5.1 回读兼容）。
  幂等：.env 不存在或没有该行同样报成功（无可清理即目标已达成）。
  注意：无需管理员权限（栈目录为当前用户可写）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\clear-frp-key.ps1
#>
[CmdletBinding()]
param(
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto',
    [string]$StackDir = 'D:\Software\cloudcli-https'
)

$ErrorActionPreference = 'Stop'

# ---------- 语言 / 输出辅助（sprint0 约定） ----------
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }
function Write-Ok   { param([string]$Message) Write-Host "[OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "[i ] $Message"  -ForegroundColor DarkGray }
function Write-Bad  { param([string]$Message) Write-Host "[X ] $Message"  -ForegroundColor Red }

# ---------- 路径常量（与 set-frp-key.ps1 对齐） ----------
$EnvFile = Join-Path $StackDir '.env'
$KeyVar  = 'SAKURA_FRP_KEY'

# ---------- 移除（幂等：无可清理即成功） ----------
if (-not (Test-Path $EnvFile)) {
    Write-Ok (T "未找到 .env（$EnvFile）——无可清理，目标已达成。" "No .env found ($EnvFile) - nothing to clean, goal already met.")
    exit 0
}
$lines = [System.IO.File]::ReadAllLines($EnvFile)
$kept = @($lines | Where-Object { $_ -notmatch "^\s*$KeyVar\s*=" })
if ($kept.Count -eq $lines.Count) {
    Write-Ok (T '.env 中未发现 SAKURA_FRP_KEY 行——无可清理，目标已达成。' 'No SAKURA_FRP_KEY line in .env - nothing to clean, goal already met.')
    exit 0
}
# UTF-8 BOM：与 set-frp-key.ps1 / set-tencent-key.ps1 的写入形态一致
$utf8Bom = [System.Text.UTF8Encoding]::new($true)
[System.IO.File]::WriteAllLines($EnvFile, $kept, $utf8Bom)

# ---------- 复核 ----------
if (Select-String -Path $EnvFile -Pattern "^\s*$KeyVar\s*=" -Quiet) {
    Write-Bad (T '复核失败：SAKURA_FRP_KEY 行仍存在，请手动检查 .env。' 'Verification failed: SAKURA_FRP_KEY still present; please check .env manually.')
    exit 1
}
Write-Ok (T "已从 .env 移除 SAKURA_FRP_KEY 行（其余行原样保留）：$EnvFile" "SAKURA_FRP_KEY line removed from .env (other lines kept): $EnvFile")
Write-Info (T '穿透通道密钥已清除。重新启用穿透时需再次运行 set-frp-key.ps1。' 'Tunnel key cleared. Run set-frp-key.ps1 again if you re-enable the tunnel channel.')
