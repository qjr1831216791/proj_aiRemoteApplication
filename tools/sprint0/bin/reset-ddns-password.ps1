<#
.SYNOPSIS
  ddns-go 管理页密码重置：停进程 → 输入新密码 → 官方通道重置 → 隐藏重启并复核。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  忘记密码时的重置脚本（ddns-go 官方 -resetPassword 通道）。无需管理员权限
  （ddns-go 为当前用户进程）。流程：
    1. 展示当前用户名（yaml 明文可读，防"用户名也忘了"）
    2. 停止运行中的 ddns-go（未运行跳过；避免运行中进程周期回写 yaml 覆盖重置）
    3. 键入新密码（不回显，两次确认，空/不一致拒绝）
    4. ddns-go -resetPassword 重置（-c 必带，防止改错 %USERPROFILE% 默认文件）
    5. 按生产参数隐藏窗重启（-c <yaml> -l :9876 -f 300），轮询端口就绪
  注意：重置瞬间密码会短暂出现在 ddns-go 进程命令行（官方接口形态，秒级窗口）；
  密码不写入任何日志。DNS 解析配置（dnsconf/密钥）不受影响。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\reset-ddns-password.ps1
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

# SecureString → 明文（仅用于传给 ddns-go -resetPassword 参数，不落任何输出）
function ConvertTo-PlainText {
    param([Security.SecureString]$Secure)
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Secure)
    try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

# ---------- 路径与参数常量（与 install-server.ps1 / APP consts.rs 对齐） ----------
$DdnsExe  = Join-Path $StackDir 'ddns-go.exe'
$DdnsYaml = Join-Path $StackDir 'ddns-go.yaml'
$DdnsPort = 9876
# 重启参数契约：-c 必带（防改错默认路径文件），-l/-f 与 APP 启动一致
$RunArgs  = @('-c', $DdnsYaml, '-l', ":$DdnsPort", '-f', '300')

# ---------- 前置检查 ----------
if (-not (Test-Path $DdnsExe)) {
    Write-Bad (T "未找到 ddns-go.exe：$DdnsExe" "ddns-go.exe not found: $DdnsExe")
    Write-Bad (T '请先运行 install-server.ps1 安装服务端。' 'Run install-server.ps1 first to install the server.')
    exit 1
}
if (-not (Test-Path $DdnsYaml)) {
    Write-Bad (T "未找到配置文件：$DdnsYaml" "Config not found: $DdnsYaml")
    Write-Bad (T '请先运行 install-server.ps1 安装服务端。' 'Run install-server.ps1 first to install the server.')
    exit 1
}

# ---------- 1. 当前用户名（只读展示） ----------
$username = $null
foreach ($line in (Get-Content $DdnsYaml)) {
    if ($line -match '^\s*username:\s*(\S.*?)\s*$') { $username = $Matches[1]; break }
}
if ($username) {
    Write-Info (T "当前用户名：$username" "Current username: $username")
} else {
    Write-Info (T '未在配置中找到用户名（可能尚未设置过）。' 'No username found in the config (may not be set yet).')
}

# ---------- 2. 停止运行中的 ddns-go ----------
$proc = Get-Process -Name 'ddns-go' -ErrorAction SilentlyContinue
if ($proc) {
    Write-Host (T '[1/4] 停止运行中的 ddns-go ...' '[1/4] Stopping the running ddns-go ...')
    $proc | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800
    if (Get-Process -Name 'ddns-go' -ErrorAction SilentlyContinue) {
        Write-Bad (T 'ddns-go 停止失败（权限或占用），已终止重置；服务未被改动。' 'Failed to stop ddns-go (permission/lock); reset aborted. Service untouched.')
        exit 1
    }
    Write-Ok (T 'ddns-go 已停止。' 'ddns-go stopped.')
} else {
    Write-Info (T '[1/4] ddns-go 未在运行，跳过停止。' '[1/4] ddns-go is not running; skipping stop.')
}

# ---------- 3. 键入并确认新密码（不回显；空/不一致拒绝） ----------
Write-Host (T '[2/4] 输入新密码（输入不回显）。' '[2/4] Enter the new password (input is hidden).')
$newPassword = $null
while ($true) {
    $sec1 = Read-Host -AsSecureString (T '新密码' 'New password')
    $sec2 = Read-Host -AsSecureString (T '再输一次确认' 'Confirm the new password')
    $p1 = ConvertTo-PlainText $sec1
    $p2 = ConvertTo-PlainText $sec2
    if ($p1.Length -eq 0) {
        Write-Bad (T '密码不能为空，请重新输入。' 'Password cannot be empty; try again.')
        continue
    }
    if ($p1 -cne $p2) {
        Write-Bad (T '两次输入不一致，请重新输入。' 'The two entries do not match; try again.')
        continue
    }
    $newPassword = $p1
    break
}

# ---------- 4. 官方通道重置 ----------
Write-Host (T '[3/4] 重置密码 ...' '[3/4] Resetting the password ...')
& $DdnsExe -c $DdnsYaml -resetPassword $newPassword 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Bad (T "重置失败（ddns-go exit $LASTEXITCODE）。" "Reset failed (ddns-go exit $LASTEXITCODE).")
    Write-Info (T '服务当前处于停止状态，可在 APP 点「启动全部」恢复。' 'Services are stopped; use Start All in the app to recover.')
    exit 1
}
Write-Ok (T '密码已重置。' 'Password has been reset.')

# ---------- 5. 按生产参数隐藏重启 + 就绪复核 ----------
Write-Host (T '[4/4] 重新启动 ddns-go ...' '[4/4] Restarting ddns-go ...')
Start-Process -FilePath $DdnsExe -ArgumentList $RunArgs -WindowStyle Hidden
$ready = $false
for ($i = 0; $i -lt 15; $i++) {
    Start-Sleep -Seconds 2
    $tcp = New-Object Net.Sockets.TcpClient
    try {
        $tcp.Connect('127.0.0.1', $DdnsPort)
        if ($tcp.Connected) { $ready = $true }
    } catch { } finally { $tcp.Close() }
    if ($ready) { break }
}
if ($ready) {
    Write-Ok (T "ddns-go 已就绪（端口 $DdnsPort 监听中）。" "ddns-go is ready (listening on port $DdnsPort).")
    Write-Ok (T '请用新密码登录管理页：http://127.0.0.1:9876/' 'Log in to the admin page with the new password: http://127.0.0.1:9876/')
    exit 0
} else {
    Write-Bad (T 'ddns-go 启动后 30s 内未就绪，重置结果未验证。' 'ddns-go did not become ready within 30s; reset result unverified.')
    Write-Info (T "手动排查：运行 & '$DdnsExe' -c '$DdnsYaml' 查看报错；或在 APP 点「启动全部」。" "Troubleshoot: run & '$DdnsExe' -c '$DdnsYaml' to see errors, or use Start All in the app.")
    exit 1
}
