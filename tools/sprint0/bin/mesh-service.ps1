<#
.SYNOPSIS
  EasyTier 组网服务管理（spec 007）：install / uninstall / start / stop / restart / status。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  管理对象为 Windows 服务 EasyTierMesh（SYSTEM 承载，delayed-auto 自启 + SCM
  崩溃自愈 restart/60000×3，spec 007 plan §4.4）：
    - install   ：注册服务 + 配自愈 + 启动。binPath 只含路径参数（-c config.toml /
                  -r RPC / --file-log-dir），network_secret 只存在于 config.toml——
                  AC8 命令行无密钥。服务已存在时按幂等更新处理（刷新 binPath 与配置）
    - uninstall ：停止并删除服务
    - start/stop/restart ：常规起停（config.toml 变更后 restart 生效）
    - status    ：服务态 / 启动类型 / binPath（只读，无需管理员）

  前置（install 前）：easytier 五文件与 config.toml 已落位到 <StackDir>\easytier\
  （工作台 mesh_install_service 命令 / 装机向导组网分支自动完成），缺失即报错指引。

  需要管理员权限的动作：install / uninstall / start / stop / restart（status 免提权）。
  工作台经 UAC（ShellExecuteW runas）拉起本脚本；手工运行请先开管理员 PowerShell
  （install-https.ps1 同惯例）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\mesh-service.ps1 -Action install
  powershell -ExecutionPolicy Bypass -File .\mesh-service.ps1 -Action restart -StackDir D:\Software\cloudcli-https
  powershell -ExecutionPolicy Bypass -File .\mesh-service.ps1 -Action status
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('install', 'uninstall', 'start', 'stop', 'restart', 'status')]
    [string]$Action,
    [string]$StackDir = 'D:\Software\cloudcli-https',
    [string]$BinPath = '',
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
function Write-Bad  { param([string]$Message) Write-Host "    [X ] $Message"  -ForegroundColor Red }

# sc.exe 退出码判定（原生 exe 不受 $ErrorActionPreference 约束；输出本地化，仅错误路径透出）
function Invoke-Sc {
    param([string[]]$ScArgs)
    $out = & sc.exe @ScArgs 2>&1
    if ($LASTEXITCODE -ne 0) { throw "sc.exe $($ScArgs -join ' ') 失败：$out" }
}

# ---------- 服务常量（plan §4.4；与 mesh.rs 的 SERVICE_NAME / RPC_PORTAL 对齐）----------
$ServiceName = 'EasyTierMesh'
$DisplayName = 'AI Remote Workbench Mesh (EasyTier)'
$RpcPortal   = '127.0.0.1:15888'
$EtDir       = Join-Path $StackDir 'easytier'
$CoreExe     = Join-Path $EtDir 'easytier-core.exe'
$ConfigFile  = Join-Path $EtDir 'config.toml'
$LogDir      = Join-Path $EtDir 'logs'

# binPath：优先采用调用方传入（工作台 mesh.rs service_bin_path() 构造并单测锁形，
# AC8 断言参数不含 secret）；未传则按同公式从 StackDir 推导（支持手工运行）
if (-not $BinPath) {
    $BinPath = "`"$CoreExe`" -c `"$ConfigFile`" -r $RpcPortal --file-log-dir `"$LogDir`""
}
if (-not $BinPath.StartsWith("`"$CoreExe`"")) {
    Write-Bad (T "binPath 与栈目录不符（应以 $CoreExe 开头）" "binPath mismatches StackDir (must start with $CoreExe)")
    exit 1
}

# ---------- 管理员检查（变更动作；status 只读免提权）----------
$mutating = @('install', 'uninstall', 'start', 'stop', 'restart') -contains $Action
if ($mutating) {
    $principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-Bad (T '请以管理员身份运行：先打开管理员 PowerShell，再执行' 'Please run as administrator: open an elevated PowerShell first, then run')
        Write-Host  "    powershell -ExecutionPolicy Bypass -File .\mesh-service.ps1 -Action $Action -StackDir $StackDir" -ForegroundColor Yellow
        exit 1
    }
}

function Get-ServiceOrNull {
    Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
}

try {
    switch ($Action) {
        'install' {
            Write-Step (T '安装组网服务' 'Install mesh service')
            # 前置：二进制与配置由工作台落位（缺失 = 未走向导/设置入口，此处不代劳下载）
            if (-not (Test-Path $CoreExe)) {
                throw (T "未找到 $CoreExe：请先在工作台「设置 → 组网」或装机向导完成安装入口（自动落位二进制）" "Missing ${CoreExe}: use the workbench Settings mesh entry or the wizard first (stages binaries automatically)")
            }
            if (-not (Test-Path $ConfigFile)) {
                throw (T "未找到 $ConfigFile：组网配置尚未生成（工作台渲染；请先用 set-mesh-secret.ps1 写入密钥）" "Missing ${ConfigFile}: mesh config not rendered yet (workbench renders it; run set-mesh-secret.ps1 first)")
            }
            New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

            $existing = Get-ServiceOrNull
            if ($existing) {
                Write-Info (T '服务已存在，按幂等更新处理（刷新 binPath 与启动/自愈配置）' 'Service exists; updating idempotently (refresh binPath, startup and recovery config)')
                if ($existing.Status -eq 'Running') { Stop-Service -Name $ServiceName }
                # CIM Change 正确承载含引号/空格的 PathName（sc.exe 转义经 PS 5.1 传参易碎）
                $svc = Get-CimInstance Win32_Service -Filter "Name='$ServiceName'"
                $null = Invoke-CimMethod -InputObject $svc -MethodName Change -Arguments @{ PathName = $BinPath }
            } else {
                New-Service -Name $ServiceName -BinaryPathName $BinPath -DisplayName $DisplayName `
                    -Description (T 'AI 远程工作台组网服务（EasyTier 虚拟网卡与节点互联）' 'AI Remote Workbench mesh service (EasyTier virtual NIC and peering)') `
                    -StartupType Automatic | Out-Null
                Write-Ok (T '服务已注册' 'Service registered')
            }
            # delayed-auto 自启 + SCM 崩溃自愈（AC3：被杀 ≤60s 拉回，不依赖工作台在跑）
            Invoke-Sc @('config', $ServiceName, 'start=', 'delayed-auto')
            Invoke-Sc @('failure', $ServiceName, 'reset=', '86400', 'actions=', 'restart/60000/restart/60000/restart/60000')
            Start-Service -Name $ServiceName
            Write-Ok (T '服务已启动（开机延迟自启 + 崩溃 60 秒内自动拉回）' 'Service started (delayed autostart + crash auto-restart within 60s)')
        }
        'uninstall' {
            Write-Step (T '卸载组网服务' 'Uninstall mesh service')
            $svc = Get-ServiceOrNull
            if (-not $svc) {
                Write-Info (T '服务不存在，无需卸载' 'Service not installed; nothing to do')
                break
            }
            if ($svc.Status -eq 'Running') {
                Stop-Service -Name $ServiceName
                Write-Ok (T '服务已停止' 'Service stopped')
            }
            Invoke-Sc @('delete', $ServiceName)
            Write-Ok (T '服务已删除' 'Service deleted')
        }
        'start' {
            Start-Service -Name $ServiceName
            Write-Ok (T '服务已启动' 'Service started')
        }
        'stop' {
            Stop-Service -Name $ServiceName
            Write-Ok (T '服务已停止（组网中断；DNS 的 A 记录仍指虚拟 IP）' 'Service stopped (mesh down; DNS A record still points to the virtual IP)')
        }
        'restart' {
            Write-Step (T '重启组网服务（使 config.toml 生效）' 'Restart mesh service (apply config.toml)')
            $svc = Get-ServiceOrNull
            if (-not $svc) { throw (T '服务未安装：请先 install' 'Service not installed; run install first') }
            if ($svc.Status -eq 'Running') { Stop-Service -Name $ServiceName }
            Start-Service -Name $ServiceName
            Write-Ok (T '服务已重启' 'Service restarted')
        }
        'status' {
            $svc = Get-ServiceOrNull
            if (-not $svc) {
                Write-Info (T '服务未安装' 'Service not installed')
                break
            }
            # CIM/注册表读取不受 sc.exe 输出本地化影响（中文系统 sc qc 标签是中文，解析易碎）
            $cim = Get-CimInstance Win32_Service -Filter "Name='$ServiceName'"
            $delayed = (Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Services\$ServiceName" -Name DelayedAutostart -ErrorAction SilentlyContinue).DelayedAutostart -eq 1
            $startType = "$($cim.StartMode)$(if ($delayed) { ' (delayed)' } else { '' })"
            Write-Host ((T '服务状态' 'Service state') + " : $($svc.Status)")
            Write-Host ((T '启动类型' 'Startup type') + " : $startType")
            Write-Host ((T '命令行'   'binPath') + " : $($cim.PathName)")
        }
    }
} catch {
    Write-Bad "$_"
    exit 1
}
