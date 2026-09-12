<#
.SYNOPSIS
  HTTPS 环境配置：443 白名单防火墙规则（经 lan-guard.ps1）+ hosts 钉定 DNS API 域名。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  需要管理员权限（enable-https.bat 会自动提权调用本脚本）。两步均可逆、幂等：
    1. 调同目录 lan-guard.ps1 -Action ensure-whitelist：创建/刷新 443 白名单规则
       "CloudCLI Mesh HTTPS 443"（入站 TCP 443 + 源 ∈ 组网虚拟网段 + 接口 = TUN
       网卡，Profile Any——spec 010 契约，取代旧 Private 归类规则）。TUN 未就绪
       （lan-guard 退出码 3）时跳过并如实提示：本步时序早于组网阶段，白名单会在
       工作台向导/设置区组网「应用」的组合派发时点就位，规则缺失不构成暴露。
    2. hosts 钉定 dnspod.tencentcloudapi.com 的 IPv4（本机 IPv6 出口异常时，
       API 解析优先走坏掉的 v6 会导致调用失败，钉 v4 兜底）

  旧版第 2 步「将所有网络配置文件设为专用」已删除（spec 010）：白名单是唯一闸门，
  与网络归类无关；归类调整回归用户自主入口（工作台「网络环境」卡）。
  3001 直访自 spec 010 起默认收口（Windows 默认拒绝），临时放行走工作台例外开关。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\enable-https.ps1
#>
[CmdletBinding()]
param(
    [string]$MeshCidr = '10.126.126.0/24',
    [string]$MeshVirtualIp = '10.126.126.1',
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
function Write-Ok   { param([string]$Message) Write-Host "[OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "[i ] $Message"  -ForegroundColor DarkGray }
function Write-Bad  { param([string]$Message) Write-Host "[X ] $Message"  -ForegroundColor Red }

# ---------- 管理员检查 ----------
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Bad (T '请以管理员身份运行（双击 enable-https.bat 会自动提权）。' 'Run as administrator (double-click enable-https.bat to auto-elevate).')
    exit 1
}

# ---------- 1. 443 白名单（lan-guard 单点承载，spec 010）----------
Write-Host (T '[1/2] 443 白名单规则（lan-guard ensure-whitelist）...' '[1/2] 443 whitelist rule (via lan-guard ensure-whitelist) ...')
$lanGuard = Join-Path $PSScriptRoot 'lan-guard.ps1'
if (-not (Test-Path $lanGuard)) {
    Write-Bad (T '缺少同目录的 lan-guard.ps1，无法配置白名单（请核对脚本目录完整）' 'lan-guard.ps1 missing next to this script; cannot set up the whitelist (check the script directory)')
    exit 1
}
# 子进程调用：lan-guard 的退出码契约（0/1/3）经 $LASTEXITCODE 如实读取
& powershell -NoProfile -ExecutionPolicy Bypass -File $lanGuard `
    -Action ensure-whitelist -Cidr $MeshCidr -VirtualIp $MeshVirtualIp -WaitTun 20 `
    -Lang $script:lang
$lgExit = $LASTEXITCODE
$lgFailed = $false
switch ($lgExit) {
    0 { Write-Ok (T '白名单已就位：成员设备可达，非成员在网络层不可达（与网络归类无关）' 'Whitelist in place: members reachable, non-members unreachable at the network layer (regardless of network category)') }
    3 { Write-Info (T 'TUN 未就绪，白名单暂不创建（休眠，非异常）——组网「应用」时会自动就位；继续后续步骤' 'TUN not ready; whitelist not created yet (dormant, not an error) - the mesh "apply" step will set it up; continuing') }
    default { $lgFailed = $true; Write-Bad (T "lan-guard 失败（exit $lgExit），白名单未就位——组网就绪后重跑本脚本" "lan-guard failed (exit $lgExit), whitelist not in place - re-run this script once the mesh is up") }
}

# ---------- 2. hosts 钉定 DNS API 域名 ----------
Write-Host (T '[2/2] hosts 钉定 dnspod.tencentcloudapi.com ...' '[2/2] Pinning dnspod.tencentcloudapi.com in hosts ...')
$hostsFile = Join-Path $env:SystemRoot 'System32\drivers\etc\hosts'
if (Select-String -Path $hostsFile -Pattern 'dnspod\.tencentcloudapi\.com' -Quiet) {
    Write-Info (T 'hosts 条目已存在 - 跳过' 'hosts entry already present - skipped')
} else {
    $ip = ([System.Net.Dns]::GetHostAddresses('dnspod.tencentcloudapi.com') |
        Where-Object { $_.AddressFamily -eq 'InterNetwork' } | Select-Object -First 1).IPAddressToString
    Add-Content -Path $hostsFile -Value ('{0} dnspod.tencentcloudapi.com' -f $ip)
    Write-Ok (T "hosts 已钉定：$ip dnspod.tencentcloudapi.com" "hosts pinned: $ip dnspod.tencentcloudapi.com")
}

Write-Host ''
if ($lgFailed) { exit 1 }
Write-Ok (T 'HTTPS 环境配置完成。' 'HTTPS environment setup done.')
Write-Info (T '若刚改过 DNS 记录，最长 10 分钟缓存过期；手机可开关一次飞行模式加速。' 'If DNS record just changed, cache expires within 10 min; toggle airplane mode on the phone to speed up.')
