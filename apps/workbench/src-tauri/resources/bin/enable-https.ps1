<#
.SYNOPSIS
  HTTPS 一次性配置：防火墙 443 + 网络改专用 + hosts 钉定 DNS API 域名。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  需要管理员权限（enable-https.bat 会自动提权调用本脚本）。三步均可逆、幂等：
    1. 创建防火墙入站规则 "CloudCLI LAN HTTPS 443"（TCP 443，仅专用网络）
    2. 将所有非域网络配置文件设为"专用"（否则 private 规则不生效）
    3. hosts 钉定 dnspod.tencentcloudapi.com 的 IPv4（本机 IPv6 出口异常时，
       API 解析优先走坏掉的 v6 会导致调用失败，钉 v4 兜底）

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\enable-https.ps1
#>
[CmdletBinding()]
param(
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

# ---------- 1. 防火墙规则 ----------
Write-Host (T '[1/3] 防火墙放行 TCP 443 ...' '[1/3] Firewall rule for TCP 443 ...')
if (Get-NetFirewallRule -DisplayName 'CloudCLI LAN HTTPS 443' -ErrorAction SilentlyContinue) {
    Write-Info (T '规则已存在：CloudCLI LAN HTTPS 443 - 跳过' 'Rule already exists: CloudCLI LAN HTTPS 443 - skipped')
} else {
    New-NetFirewallRule -DisplayName 'CloudCLI LAN HTTPS 443' -Direction Inbound -Protocol TCP `
        -LocalPort 443 -Action Allow -Profile Private | Out-Null
    Write-Ok (T '已创建规则：TCP 443 放行（专用网络）' 'Rule created: TCP 443 allowed on private networks')
}

# ---------- 2. 网络配置文件 -> 专用 ----------
Write-Host (T '[2/3] 网络配置文件 -> 专用 ...' '[2/3] Setting network profiles to Private ...')
Get-NetConnectionProfile | Where-Object { $_.NetworkCategory -ne 'Private' -and $_.NetworkCategory -ne 'DomainAuthenticated' } |
    ForEach-Object {
        Set-NetConnectionProfile -InterfaceIndex $_.InterfaceIndex -NetworkCategory Private
        Write-Ok "$($_.Name) -> Private"
    }

# ---------- 3. hosts 钉定 DNS API 域名 ----------
Write-Host (T '[3/3] hosts 钉定 dnspod.tencentcloudapi.com ...' '[3/3] Pinning dnspod.tencentcloudapi.com in hosts ...')
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
Write-Ok (T 'HTTPS 环境配置完成。' 'HTTPS environment setup done.')
Write-Info (T '若刚改过 DNS 记录，最长 10 分钟缓存过期；手机可开关一次飞行模式加速。' 'If DNS record just changed, cache expires within 10 min; toggle airplane mode on the phone to speed up.')
