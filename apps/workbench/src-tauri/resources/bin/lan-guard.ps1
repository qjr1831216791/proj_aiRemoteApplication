<#
.SYNOPSIS
  LAN 边界防火墙守卫（spec 010）：防火墙规则 CRUD 的单点脚本，五动作幂等。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  规则契约（spec 010 plan §3.2，T1 冻结）：
    - 443 白名单  CloudCLI Mesh HTTPS 443     入站 TCP 443 + RemoteAddress=组网
      虚拟网段 + InterfaceAlias=TUN 网卡（运行期解析），Profile Any——成员（持
      密钥经 TUN 以虚拟 IP 到达）可达，非成员/伪造源 IP 从物理网卡进入即不匹配，
      网络层不可达（唯一闸门，与网络归类无关）。
    - 3001 例外   CloudCLI LAN 3001 Exception  入站 TCP 3001 + Profile Private +
      RemoteAddress LocalSubnet（无接口条件）——默认不存在，显式开关临时放行，
      12h 自动回落由工作台 watcher 承载（本脚本不计时）。
    - 旧规则退役  CloudCLI LAN HTTPS 443 / CloudCLI LAN 3001（migrate 幂等删除）。

  动作（plan §5.1）：
    status            只读探测，输出压缩 JSON（plan §4.2；tun=null=休眠态），免提权
    ensure-whitelist  443 白名单幂等就位：已存在且 RemoteAddress+InterfaceAlias
                      均匹配 -> 跳过；失配或不存在 -> Remove+New
    exception-on      3001 例外开启（同形幂等：匹配跳过 / 失配重建 / 缺失新建）
    exception-off     3001 例外关闭（仅删，不存在亦成功）
    migrate           存量迁移：删两条旧规则（幂等）-> ensure-whitelist 逻辑复用

  退出码（工作台以 status 复测为准，本契约供命令行手工排查）：
    0 = 成功/幂等跳过；1 = 前置不满足（非管理员/参数非法）；
    3 = TUN 未解析（-WaitTun 等待耗尽，不创建规则——组网不在时成员本就无 TUN
        路由，规则缺失不构成暴露，属「休眠」而非异常）

  TUN 解析（plan §3.3）：以「持有 -VirtualIp 的 IPv4 适配器」为语义锚点——
  wintun 接口名（et_*）按实例生成，不可作契约常量。

  需要管理员权限的动作：ensure-whitelist / exception-on / exception-off / migrate
  （status 免提权）。工作台经 UAC（ShellExecuteW runas + -WindowStyle Hidden）
  派发本脚本；手工运行请先开管理员 PowerShell（mesh-service.ps1 同惯例）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action status -VirtualIp 10.126.126.1
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action ensure-whitelist -Cidr 10.126.126.0/24 -VirtualIp 10.126.126.1 -WaitTun 20
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action exception-on
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action exception-off
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action migrate -Cidr 10.126.126.0/24 -VirtualIp 10.126.126.1
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('status', 'ensure-whitelist', 'exception-on', 'exception-off', 'migrate')]
    [string]$Action,
    [string]$Cidr = '10.126.126.0/24',
    [string]$VirtualIp = '',
    [int]$WaitTun = 0,
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

$ErrorActionPreference = 'Stop'
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { <# 无控制台环境不阻断 #> }

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

# ---------- 规则契约常量（T1 冻结；与 src/lan_guard.rs 常量对齐，漂移即单测失配）----------
$RuleMesh443    = 'CloudCLI Mesh HTTPS 443'
$RuleExc3001    = 'CloudCLI LAN 3001 Exception'
$LegacyRule443  = 'CloudCLI LAN HTTPS 443'
$LegacyRule3001 = 'CloudCLI LAN 3001'

# ---------- 参数体检（非法 -> exit 1；status 的 -VirtualIp 缺省容忍 = 休眠态）----------
$cidrRe = '^\d{1,3}(\.\d{1,3}){3}/\d{1,2}$'
$ipRe   = '^\d{1,3}(\.\d{1,3}){3}$'
$needsCidr = $Action -in @('ensure-whitelist', 'migrate')
if ($needsCidr -and $Cidr -notmatch $cidrRe) {
    Write-Bad (T "参数非法：-Cidr 应为 x.x.x.x/nn（收到：$Cidr）" "Invalid -Cidr (expected x.x.x.x/nn, got: $Cidr)")
    exit 1
}
if ($needsCidr -and $VirtualIp -notmatch $ipRe) {
    Write-Bad (T "参数非法：-VirtualIp 应为 IPv4（收到：'$VirtualIp'）" "Invalid -VirtualIp (expected IPv4, got: '$VirtualIp')")
    exit 1
}
if ($Action -eq 'status' -and $VirtualIp -and ($VirtualIp -notmatch $ipRe)) {
    Write-Bad (T "参数非法：-VirtualIp 应为 IPv4（收到：'$VirtualIp'）" "Invalid -VirtualIp (expected IPv4, got: '$VirtualIp')")
    exit 1
}

# ---------- 管理员检查（变更动作；status 只读免提权）----------
if ($Action -ne 'status') {
    $principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-Bad (T '请以管理员身份运行：先打开管理员 PowerShell，再执行' 'Please run as administrator: open an elevated PowerShell first, then run')
        Write-Host  "    powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action $Action -Cidr $Cidr -VirtualIp $VirtualIp" -ForegroundColor Yellow
        exit 1
    }
}

# ---------- TUN 解析（plan §3.3：持有 -VirtualIp 的 IPv4 适配器 = TUN）----------
function Get-TunAlias {
    param([string]$Vip)
    if (-not $Vip) { return $null }
    $a = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
        Where-Object { $_.IPAddress -eq $Vip } | Select-Object -First 1
    if ($a) { $a.InterfaceAlias } else { $null }
}

function Resolve-TunWithWait {
    param([string]$Vip, [int]$Seconds)
    $alias = Get-TunAlias -Vip $Vip
    for ($i = 0; (-not $alias) -and ($i -lt $Seconds); $i++) {
        Start-Sleep -Seconds 1
        $alias = Get-TunAlias -Vip $Vip
    }
    $alias
}

# RemoteAddress 条目归一为 x.x.x.x/nn（Windows 存储把 CIDR 规范成掩码形态：
# 写入 10.126.126.0/24 读回 10.126.126.0/255.255.255.0——幂等比对与 status 契约
# 均按 CIDR 形态，plan §4.2）
function Convert-ToCidrForm {
    param([string]$Entry)
    if (-not $Entry) { return $Entry }
    $parts = $Entry -split '/'
    if ($parts.Count -ne 2) { return $Entry }
    if ($parts[1] -notmatch '^\d{1,3}(\.\d{1,3}){3}$') { return $Entry }   # 已是 /nn 或无掩码
    $bits = 0; $seenZero = $false
    foreach ($b in ($parts[1] -split '\.' | ForEach-Object { [int]$_ })) {
        for ($i = 7; $i -ge 0; $i--) {
            if ($seenZero) { continue }
            if (($b -band (1 -shl $i)) -ne 0) { $bits++ } else { $seenZero = $true }
        }
    }
    "{0}/{1}" -f $parts[0], $bits
}

# 443 白名单幂等就位（ensure-whitelist / migrate 复用；TUN 未解析 -> exit 3 不建规则）
function Invoke-EnsureWhitelist {
    param([string]$C, [string]$Vip, [int]$Wait)
    $alias = Resolve-TunWithWait -Vip $Vip -Seconds $Wait
    if (-not $alias) {
        Write-Bad (T "TUN 接口未解析到（无持有 $Vip 的 IPv4 适配器，已等待 ${Wait}s）——不创建白名单（休眠态：组网不在时成员本就无 TUN 路由，规则缺失不构成暴露）" "TUN adapter holding $Vip not found (waited ${Wait}s) - whitelist NOT created (dormant: without the mesh, members have no TUN route, so a missing rule is no exposure)")
        exit 3
    }
    $existing = Get-NetFirewallRule -DisplayName $RuleMesh443 -ErrorAction SilentlyContinue
    if ($existing) {
        $af  = $existing | Get-NetFirewallAddressFilter
        $iff = $existing | Get-NetFirewallInterfaceFilter
        $remoteCidr = (@($af.RemoteAddress) | ForEach-Object { Convert-ToCidrForm $_ })
        if (($remoteCidr -contains $C) -and (@($iff.InterfaceAlias) -contains $alias)) {
            Write-Ok (T "白名单已就位：$RuleMesh443（RemoteAddress=$C，InterfaceAlias=$alias）- 跳过" "Whitelist already in place: $RuleMesh443 (RemoteAddress=$C, InterfaceAlias=$alias) - skipped")
            return
        }
        Write-Info (T "白名单失配（RemoteAddress=$($remoteCidr -join ',')，InterfaceAlias=$(@($iff.InterfaceAlias) -join ',')）-> Remove+New 重建" "Whitelist mismatch (RemoteAddress=$($remoteCidr -join ','), InterfaceAlias=$(@($iff.InterfaceAlias) -join ',')) -> Remove+New")
        Remove-NetFirewallRule -DisplayName $RuleMesh443
    }
    # Profile 不传 = Any：白名单是唯一闸门，与网络归类无关（spec AC4）
    New-NetFirewallRule -DisplayName $RuleMesh443 -Direction Inbound -Protocol TCP `
        -LocalPort 443 -Action Allow -RemoteAddress $C -InterfaceAlias $alias | Out-Null
    Write-Ok (T "白名单已就位：$RuleMesh443（TCP 443，源=$C，接口=$alias，Profile=Any）" "Whitelist created: $RuleMesh443 (TCP 443, source=$C, iface=$alias, Profile=Any)")
}

try {
    switch ($Action) {
        'status' {
            $tun = $null
            if ($VirtualIp) {
                $a = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
                    Where-Object { $_.IPAddress -eq $VirtualIp } | Select-Object -First 1
                if ($a) { $tun = [ordered]@{ name = $a.InterfaceAlias; ip = $a.IPAddress } }
            }
            $mesh = Get-NetFirewallRule -DisplayName $RuleMesh443 -ErrorAction SilentlyContinue
            $meshObj = if ($mesh) {
                $af  = $mesh | Get-NetFirewallAddressFilter
                $iff = $mesh | Get-NetFirewallInterfaceFilter
                [ordered]@{
                    present = $true
                    remote  = (@($af.RemoteAddress) | ForEach-Object { Convert-ToCidrForm $_ }) -join ','
                    iface   = @($iff.InterfaceAlias) -join ','
                    profile = [int]$mesh.Profile
                }
            } else {
                [ordered]@{ present = $false; remote = $null; iface = $null; profile = 0 }
            }
            $exc = Get-NetFirewallRule -DisplayName $RuleExc3001 -ErrorAction SilentlyContinue
            $excObj = if ($exc) {
                [ordered]@{ present = $true; profile = [int]$exc.Profile }
            } else {
                [ordered]@{ present = $false; profile = 0 }
            }
            # 输出契约（plan §4.2）：压缩 JSON；stdout 仅此一行（Write-Host 走信息流不污染管道）
            [ordered]@{
                legacy443  = [bool](Get-NetFirewallRule -DisplayName $LegacyRule443 -ErrorAction SilentlyContinue)
                legacy3001 = [bool](Get-NetFirewallRule -DisplayName $LegacyRule3001 -ErrorAction SilentlyContinue)
                mesh443    = $meshObj
                exc3001    = $excObj
                tun        = $tun
            } | ConvertTo-Json -Compress -Depth 4
            exit 0
        }
        'ensure-whitelist' {
            Write-Step (T '443 白名单幂等就位（源 ∈ 组网虚拟网段 + TUN 接口，Profile Any）' 'Ensure the 443 whitelist (source in mesh CIDR + TUN interface, Profile Any)')
            Invoke-EnsureWhitelist -C $Cidr -Vip $VirtualIp -Wait $WaitTun
        }
        'exception-on' {
            Write-Step (T '开启 3001 局域网例外（Private + LocalSubnet，无接口条件）' 'Enable the 3001 LAN exception (Private + LocalSubnet, no interface condition)')
            $existing = Get-NetFirewallRule -DisplayName $RuleExc3001 -ErrorAction SilentlyContinue
            if ($existing) {
                $af  = $existing | Get-NetFirewallAddressFilter
                $iff = $existing | Get-NetFirewallInterfaceFilter
                $profileOk = (([int]$existing.Profile -band 2) -ne 0)   # 位掩码 2 = Private
                $remoteOk  = @($af.RemoteAddress) -contains 'LocalSubnet'
                $noIface   = ((@($iff.InterfaceAlias).Count -eq 0) -or (@($iff.InterfaceAlias) -contains 'Any'))
                if ($profileOk -and $remoteOk -and $noIface) {
                    Write-Ok (T "例外已开启且匹配：$RuleExc3001 - 跳过" "Exception already on and matching: $RuleExc3001 - skipped")
                    break
                }
                Write-Info (T "例外规则失配（Profile=$($existing.Profile)，Remote=$(@($af.RemoteAddress) -join ',')，IfAlias=$(@($iff.InterfaceAlias) -join ',')）-> Remove+New 重建" "Exception rule mismatch (Profile=$($existing.Profile), Remote=$(@($af.RemoteAddress) -join ','), IfAlias=$(@($iff.InterfaceAlias) -join ',')) -> Remove+New")
                Remove-NetFirewallRule -DisplayName $RuleExc3001
            }
            New-NetFirewallRule -DisplayName $RuleExc3001 -Direction Inbound -Protocol TCP `
                -LocalPort 3001 -Action Allow -Profile Private -RemoteAddress LocalSubnet | Out-Null
            Write-Ok (T "例外已开启：$RuleExc3001（TCP 3001，仅专用网络 + 本机子网；12h 自动回落由工作台承载）" "Exception on: $RuleExc3001 (TCP 3001, private networks + local subnet only; the 12h auto-revert is handled by the workbench)")
        }
        'exception-off' {
            Write-Step (T '关闭 3001 局域网例外（不存在亦成功）' 'Disable the 3001 LAN exception (absent counts as success)')
            if (Get-NetFirewallRule -DisplayName $RuleExc3001 -ErrorAction SilentlyContinue) {
                Remove-NetFirewallRule -DisplayName $RuleExc3001
                Write-Ok (T "例外规则已删除：$RuleExc3001（回落 Windows 默认拒绝）" "Exception rule removed: $RuleExc3001 (back to Windows default-deny)")
            } else {
                Write-Info (T '例外规则不存在（本就关闭）- 无需操作' 'Exception rule absent (already off) - nothing to do')
            }
        }
        'migrate' {
            Write-Step (T '存量迁移：删除旧规则并就位 443 白名单（幂等）' 'Migrate: remove legacy rules and set up the 443 whitelist (idempotent)')
            foreach ($legacy in @($LegacyRule443, $LegacyRule3001)) {
                if (Get-NetFirewallRule -DisplayName $legacy -ErrorAction SilentlyContinue) {
                    Remove-NetFirewallRule -DisplayName $legacy
                    Write-Ok (T "旧规则已删除：$legacy" "Legacy rule removed: $legacy")
                } else {
                    Write-Info (T "旧规则不存在（幂等跳过）：$legacy" "Legacy rule absent (idempotent skip): $legacy")
                }
            }
            Invoke-EnsureWhitelist -C $Cidr -Vip $VirtualIp -Wait $WaitTun
        }
    }
} catch {
    Write-Bad "$_"
    exit 1
}
