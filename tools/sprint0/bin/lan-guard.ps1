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
    - 程序级旁路清理（AC11，spec §4「程序级规则清理」）：Windows 首次运行弹窗
      会按 exe 自动创建「全端口 × 任意 profile」的程序级入站 Allow 规则，完全
      绕过端口白名单契约——ensure-whitelist / migrate 尾部同步清理服务进程的
      程序规则（与 TUN/白名单解耦，TUN 未就绪也照常清理）：
        * CloudCLI node：从 3001 监听进程取实际可执行路径精确识别（含符号链接
          解析出的真实形态；进程不在 -> 跳过并标注，绝不按 DisplayName 全局删）；
        * Caddy：<StackDir>\caddy.exe（另含 443 监听进程路径）；
        * easytier-core.exe（<StackDir>\easytier\）：不删除，收紧——删除全端口
          放行，重建仅限 11010（TCP/UDP 各一条，P2P 物理层保留 network_secret
          认证，spec §2 非目标「不动 11010」的端口限定替代全放行）；
        * ddns-go：任意路径的残留规则直接删除（exe 已随 008 卸载；进程仍在跑
          则跳过并标注）。
      误伤红线：只按 Program 路径精确匹配（大小写不敏感），只动「入站 + Allow」，
      机器上 wemailnode/Electron 等无关软件的程序规则绝不触碰（判定规则与
      src/lan_guard.rs classify_program_rule 纯函数镜像，漂移即单测失配）。

  动作（plan §5.1）：
    status            只读探测，输出压缩 JSON（plan §4.2 + AC11 bypass 字段；
                      tun=null=休眠态），免提权
    ensure-whitelist  443 白名单幂等就位：已存在且 RemoteAddress+InterfaceAlias
                      均匹配 -> 跳过；失配或不存在 -> Remove+New；尾部程序规则清理
    exception-on      3001 例外开启（同形幂等：匹配跳过 / 失配重建 / 缺失新建）
    exception-off     3001 例外关闭（仅删，不存在亦成功）
    migrate           存量迁移：删两条旧规则（幂等）-> ensure-whitelist 逻辑复用
                      -> 尾部程序规则清理

  退出码（工作台以 status 复测为准，本契约供命令行手工排查）：
    0 = 成功/幂等跳过；1 = 前置不满足（非管理员/参数非法）；
    3 = TUN 未解析（-WaitTun 等待耗尽，不创建规则——组网不在时成员本就无 TUN
        路由，规则缺失不构成暴露，属「休眠」而非异常；程序规则清理先行完成）

  TUN 解析（plan §3.3）：以「持有 -VirtualIp 的 IPv4 适配器」为语义锚点——
  wintun 接口名（et_*）按实例生成，不可作契约常量。

  需要管理员权限的动作：ensure-whitelist / exception-on / exception-off / migrate
  （status 免提权）。工作台经 UAC（ShellExecuteW runas + -WindowStyle Hidden）
  派发本脚本；手工运行请先开管理员 PowerShell（mesh-service.ps1 同惯例）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action status -VirtualIp 10.126.126.1 -StackDir D:\Software\cloudcli-https
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action ensure-whitelist -Cidr 10.126.126.0/24 -VirtualIp 10.126.126.1 -WaitTun 20 -StackDir D:\Software\cloudcli-https
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action exception-on
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action exception-off
  powershell -ExecutionPolicy Bypass -File .\lan-guard.ps1 -Action migrate -Cidr 10.126.126.0/24 -VirtualIp 10.126.126.1 -StackDir D:\Software\cloudcli-https
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('status', 'ensure-whitelist', 'exception-on', 'exception-off', 'migrate')]
    [string]$Action,
    [string]$Cidr = '10.126.126.0/24',
    [string]$VirtualIp = '',
    [int]$WaitTun = 0,
    [string]$StackDir = 'D:\Software\cloudcli-https',
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

# —— 程序级规则清理契约（AC11 旁路加固；与 src/lan_guard.rs 常量对齐）——
# EasyTier P2P 物理层端口（spec §2 非目标「不动 11010」：程序规则收紧后唯一保留端口）
$EtP2pPort  = 11010
$EtTightTcp = 'CloudCLI Mesh EasyTier 11010 TCP'
$EtTightUdp = 'CloudCLI Mesh EasyTier 11010 UDP'
# ddns-go 残留识别文件名（任意目录；exe 已随 008 卸载，规则无主）
$DdnsGoLeaf = 'ddns-go.exe'

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

# ---------- 程序级规则识别（AC11 旁路加固的共用底座）----------
# 监听指定端口的进程的可执行路径（进程不在/路径不可读 -> $null，调用方标注 skip）
function Get-ListenerProgramPath {
    param([int]$Port)
    try {
        $conn = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue |
            Where-Object { $_.OwningProcess } | Select-Object -First 1
        if (-not $conn) { return $null }
        $p = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
        if ($p -and $p.Path) { return $p.Path }
    } catch { }
    return $null
}

# 进程上报路径的真实形态（符号链接/junction 解析——nvm 的 nodejs 目录是
# symlink，监听进程上报链接形态而防火墙规则按链接目标落库，两种形态都算命中面）
function Resolve-RealProgramPath {
    param([string]$Path)
    if (-not $Path) { return $null }
    try {
        $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        if ($item.Target) { return [string]($item.Target | Select-Object -First 1) }
    } catch { }
    return $Path
}

# Program 精确等于 programPath 的「入站 Allow」规则（-Program 参数是通配语义，
# 取回后按精确相等复核防误匹配——'*node.exe' 通配会把 WeMailNode.exe 一并带出，
# 复核是误伤防线的一半；方向/动作过滤是另一半：出站与 Block 一律不碰）
function Get-InboundAllowProgramRules {
    param([string]$ProgramPath)
    if (-not $ProgramPath) { return @() }
    $out = @()
    foreach ($f in @(Get-NetFirewallApplicationFilter -Program $ProgramPath -ErrorAction SilentlyContinue)) {
        if (-not $f.Program -or ($f.Program -ne $ProgramPath)) { continue }
        $rule = $f | Get-NetFirewallRule -ErrorAction SilentlyContinue
        if ($rule -and $rule.Direction -eq 'Inbound' -and $rule.Action -eq 'Allow') { $out += $rule }
    }
    return $out
}

# 可执行文件名（任意目录）命中的「入站 Allow」规则（ddns-go 残留专用：任意路径
# 的 ddns-go.exe；精确复核叶名，同款方向/动作过滤）
function Get-InboundAllowProgramRulesByLeaf {
    param([string]$LeafName)
    $out = @()
    foreach ($f in @(Get-NetFirewallApplicationFilter -Program ('*' + $LeafName) -ErrorAction SilentlyContinue)) {
        if (-not $f.Program) { continue }
        if ((Split-Path $f.Program -Leaf) -ne $LeafName) { continue }
        $rule = $f | Get-NetFirewallRule -ErrorAction SilentlyContinue
        if ($rule -and $rule.Direction -eq 'Inbound' -and $rule.Action -eq 'Allow') { $out += $rule }
    }
    return $out
}

# EasyTier 规则集是否存在非收紧形态（「11010 限定的 TCP/UDP 两条」之外即宽放行；
# status 的 easytierWide 探测与清理的保留判定同源）
function Test-EasyTierWide {
    param([object[]]$Rules)
    foreach ($r in @($Rules)) {
        $pf = $r | Get-NetFirewallPortFilter
        $portOk = @($pf.LocalPort) -contains [string]$EtP2pPort
        $isTcp  = $portOk -and ($r.DisplayName -eq $EtTightTcp) -and ($pf.Protocol -eq 'TCP')
        $isUdp  = $portOk -and ($r.DisplayName -eq $EtTightUdp) -and ($pf.Protocol -eq 'UDP')
        if (-not ($isTcp -or $isUdp)) { return $true }
    }
    return $false
}

# 443 白名单幂等就位（ensure-whitelist / migrate 复用；TUN 未解析 -> 返回 $false
# 由调用方 exit 3——程序规则清理与 TUN 解耦，须先行完成）
function Invoke-EnsureWhitelist {
    param([string]$C, [string]$Vip, [int]$Wait)
    $alias = Resolve-TunWithWait -Vip $Vip -Seconds $Wait
    if (-not $alias) {
        Write-Bad (T "TUN 接口未解析到（无持有 $Vip 的 IPv4 适配器，已等待 ${Wait}s）——不创建白名单（休眠态：组网不在时成员本就无 TUN 路由，规则缺失不构成暴露）" "TUN adapter holding $Vip not found (waited ${Wait}s) - whitelist NOT created (dormant: without the mesh, members have no TUN route, so a missing rule is no exposure)")
        return $false
    }
    $existing = Get-NetFirewallRule -DisplayName $RuleMesh443 -ErrorAction SilentlyContinue
    if ($existing) {
        $af  = $existing | Get-NetFirewallAddressFilter
        $iff = $existing | Get-NetFirewallInterfaceFilter
        $remoteCidr = (@($af.RemoteAddress) | ForEach-Object { Convert-ToCidrForm $_ })
        if (($remoteCidr -contains $C) -and (@($iff.InterfaceAlias) -contains $alias)) {
            Write-Ok (T "白名单已就位：$RuleMesh443（RemoteAddress=$C，InterfaceAlias=$alias）- 跳过" "Whitelist already in place: $RuleMesh443 (RemoteAddress=$C, InterfaceAlias=$alias) - skipped")
            return $true
        }
        Write-Info (T "白名单失配（RemoteAddress=$($remoteCidr -join ',')，InterfaceAlias=$(@($iff.InterfaceAlias) -join ',')）-> Remove+New 重建" "Whitelist mismatch (RemoteAddress=$($remoteCidr -join ','), InterfaceAlias=$(@($iff.InterfaceAlias) -join ',')) -> Remove+New")
        Remove-NetFirewallRule -DisplayName $RuleMesh443
    }
    # Profile 不传 = Any：白名单是唯一闸门，与网络归类无关（spec AC4）
    New-NetFirewallRule -DisplayName $RuleMesh443 -Direction Inbound -Protocol TCP `
        -LocalPort 443 -Action Allow -RemoteAddress $C -InterfaceAlias $alias | Out-Null
    Write-Ok (T "白名单已就位：$RuleMesh443（TCP 443，源=$C，接口=$alias，Profile=Any）" "Whitelist created: $RuleMesh443 (TCP 443, source=$C, iface=$alias, Profile=Any)")
    return $true
}

# 程序规则清理（AC11；ensure-whitelist / migrate 尾部调用，幂等）：删除
# node/caddy/ddns-go 的旁路放行，easytier 收紧至 11010。逐条 [OK]/[SKIP] 明细。
function Invoke-BypassCleanup {
    param([string]$Stack)
    Write-Step (T '程序级规则清理（旁路加固）：服务进程全端口放行 -> 删除/收紧到白名单契约' 'Program-rule cleanup (bypass hardening): service-exe all-port allows -> removed/tightened')
    $removed = 0; $tightened = 0; $skipped = @()

    # a) CloudCLI node：3001 监听进程的实际路径精确识别（含符号链接真实形态）；
    #    进程不在 -> 跳过并标注（绝不按 DisplayName 全局删——防误伤其他 node 程序）
    $nodePath = Get-ListenerProgramPath -Port 3001
    if ($nodePath) {
        $nodeReal = Resolve-RealProgramPath -Path $nodePath
        $nodeForms = @($nodePath)
        if ($nodeReal -and ($nodeForms -notcontains $nodeReal)) { $nodeForms += $nodeReal }
        $nodeHit = 0
        foreach ($nf in $nodeForms) {
            foreach ($r in @(Get-InboundAllowProgramRules -ProgramPath $nf)) {
                Remove-NetFirewallRule -Name $r.Name
                Write-Ok (T "已删除旁路放行：$($r.DisplayName)（Program=$nf，全端口放行与端口白名单契约冲突）" "Removed bypass allow: $($r.DisplayName) (Program=$nf, all-port allow conflicts with the port-whitelist contract)")
                $removed++; $nodeHit++
            }
        }
        if ($nodeHit -eq 0) {
            Write-Ok (T "CloudCLI node（$($nodeForms -join ' | ')）无旁路程序规则（幂等跳过）" "No bypass program rules for the CloudCLI node ($($nodeForms -join ' | ')) (idempotent skip)")
        }
    } else {
        $skipped += (T 'node：3001 未监听，取不到实际程序路径（不按 DisplayName 猜测，防误伤）' 'node: port 3001 not listening, actual exe path unknown (never guessed by display name)')
    }

    # b) Caddy：<StackDir>\caddy.exe（另含 443 监听进程路径——栈目录迁过址时兜底）
    $caddyForms = @(Join-Path $Stack 'caddy.exe')
    $caddyListen = Get-ListenerProgramPath -Port 443
    if ($caddyListen -and ($caddyForms -notcontains $caddyListen)) { $caddyForms += $caddyListen }
    $caddyHit = 0
    foreach ($cf in $caddyForms) {
        foreach ($r in @(Get-InboundAllowProgramRules -ProgramPath $cf)) {
            Remove-NetFirewallRule -Name $r.Name
            Write-Ok (T "已删除旁路放行：$($r.DisplayName)（Program=$cf，443 由端口白名单规则承载）" "Removed bypass allow: $($r.DisplayName) (Program=$cf; 443 is covered by the port-whitelist rule)")
            $removed++; $caddyHit++
        }
    }
    if ($caddyHit -eq 0) {
        Write-Ok (T 'Caddy 无旁路程序规则（幂等跳过）' 'No bypass program rules for Caddy (idempotent skip)')
    }

    # c) EasyTier（<StackDir>\easytier\easytier-core.exe）：不删除，收紧——删除
    #    全端口放行，重建仅限 11010（TCP/UDP 各一条；P2P 物理层有 network_secret
    #    认证，端口限定替代全放行）
    $etPath = Join-Path (Join-Path $Stack 'easytier') 'easytier-core.exe'
    $etRules = @(Get-InboundAllowProgramRules -ProgramPath $etPath)
    $haveTcp = $false; $haveUdp = $false
    foreach ($r in $etRules) {
        $pf = $r | Get-NetFirewallPortFilter
        $portOk = @($pf.LocalPort) -contains [string]$EtP2pPort
        if ($portOk -and ($r.DisplayName -eq $EtTightTcp) -and ($pf.Protocol -eq 'TCP')) { $haveTcp = $true; continue }
        if ($portOk -and ($r.DisplayName -eq $EtTightUdp) -and ($pf.Protocol -eq 'UDP')) { $haveUdp = $true; continue }
        Remove-NetFirewallRule -Name $r.Name
        Write-Ok (T "已收紧旁路放行：$($r.DisplayName)（EasyTier 全端口放行 -> 仅保留 $EtP2pPort P2P 端口）" "Tightened bypass allow: $($r.DisplayName) (EasyTier all-port -> 11010 P2P port only)")
        $tightened++
    }
    if (Test-Path -LiteralPath $etPath -ErrorAction SilentlyContinue) {
        foreach ($spec in @(@{ n = $EtTightTcp; p = 'TCP' }, @{ n = $EtTightUdp; p = 'UDP' })) {
            $have = if ($spec.p -eq 'TCP') { $haveTcp } else { $haveUdp }
            if ($have) {
                Write-Ok (T "收紧规则已在位：$($spec.n)（$($spec.p) $EtP2pPort）- 跳过" "Tight rule already in place: $($spec.n) ($($spec.p) $EtP2pPort) - skipped")
                continue
            }
            New-NetFirewallRule -DisplayName $spec.n -Direction Inbound -Protocol $spec.p `
                -LocalPort $EtP2pPort -Action Allow -Program $etPath -Profile Domain,Private,Public | Out-Null
            Write-Ok (T "已收紧：$($spec.n)（仅 $($spec.p) $EtP2pPort，P2P 物理层保留 network_secret 认证）" "Tightened: $($spec.n) ($($spec.p) $EtP2pPort only; the P2P layer keeps its network_secret auth)")
            $tightened++
        }
    } elseif ($etRules.Count -gt 0) {
        $skipped += (T "easytier：栈内未找到 $etPath，宽放行已删但不重建 11010 限定规则" "easytier: $etPath not found in the stack; wide rules removed, 11010 tight rules not recreated")
    }

    # d) ddns-go 残留：任意路径的 ddns-go.exe 入站 Allow 规则直接删除（exe 已随
    #    008 卸载，规则无主）；进程仍在跑则跳过并标注
    if (Get-Process -Name 'ddns-go' -ErrorAction SilentlyContinue) {
        $skipped += (T 'ddns-go：进程仍在运行，其规则不删（请先卸载 ddns-go 再重跑）' 'ddns-go: process still running, its rules are kept (uninstall ddns-go first, then re-run)')
    } else {
        $ddnsRules = @(Get-InboundAllowProgramRulesByLeaf -LeafName $DdnsGoLeaf)
        foreach ($r in $ddnsRules) {
            Remove-NetFirewallRule -Name $r.Name
            Write-Ok (T "已删除残留旁路放行：$($r.DisplayName)（ddns-go exe 已随 008 卸载，规则无主残留）" "Removed stale bypass allow: $($r.DisplayName) (ddns-go.exe was uninstalled in 008; the rule is ownerless residue)")
            $removed++
        }
        if (-not $ddnsRules) {
            Write-Ok (T 'ddns-go 无残留程序规则（幂等跳过）' 'No stale ddns-go program rules (idempotent skip)')
        }
    }

    foreach ($s in $skipped) { Write-Info (T "跳过：$s" "Skipped: $s") }
    if (($removed -eq 0) -and ($tightened -eq 0)) {
        Write-Ok (T '程序规则清理完成：无旁路残留（幂等）' 'Program-rule cleanup done: no bypass residue (idempotent)')
    } else {
        Write-Ok (T "程序规则清理完成：删除 $removed 条、收紧 $tightened 处（EasyTier 保留 $EtP2pPort P2P）" "Program-rule cleanup done: $removed removed, $tightened tightened (EasyTier keeps $EtP2pPort P2P)")
    }
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
            # 程序级旁路残留探测（AC11）：存在指向服务程序路径的入站 Allow 程序规则
            # 即为残留。node 探测路径同清理逻辑（3001 监听进程实际路径 + 符号链接
            # 真实形态；进程不在 -> false + nodeSkipped 标注）；easytierWide =
            # 存在非「11010 限定」形态的规则；ddns-go 按任意路径叶名匹配。
            $nodePath = Get-ListenerProgramPath -Port 3001
            $nodeSkipped = (-not $nodePath)
            $nodeBypass = $false
            if ($nodePath) {
                $nodeReal = Resolve-RealProgramPath -Path $nodePath
                $nodeForms = @($nodePath)
                if ($nodeReal -and ($nodeForms -notcontains $nodeReal)) { $nodeForms += $nodeReal }
                foreach ($nf in $nodeForms) {
                    if (@(Get-InboundAllowProgramRules -ProgramPath $nf).Count -gt 0) { $nodeBypass = $true }
                }
            }
            $caddyBypass = $false
            $caddyForms = @(Join-Path $StackDir 'caddy.exe')
            $caddyListen = Get-ListenerProgramPath -Port 443
            if ($caddyListen -and ($caddyForms -notcontains $caddyListen)) { $caddyForms += $caddyListen }
            foreach ($cf in $caddyForms) {
                if (@(Get-InboundAllowProgramRules -ProgramPath $cf).Count -gt 0) { $caddyBypass = $true }
            }
            $etPath = Join-Path (Join-Path $StackDir 'easytier') 'easytier-core.exe'
            $etWide = Test-EasyTierWide -Rules (Get-InboundAllowProgramRules -ProgramPath $etPath)
            $ddnsBypass = @(Get-InboundAllowProgramRulesByLeaf -LeafName $DdnsGoLeaf).Count -gt 0
            # 输出契约（plan §4.2 + AC11）：压缩 JSON；stdout 仅此一行（Write-Host 走信息流不污染管道）
            [ordered]@{
                legacy443  = [bool](Get-NetFirewallRule -DisplayName $LegacyRule443 -ErrorAction SilentlyContinue)
                legacy3001 = [bool](Get-NetFirewallRule -DisplayName $LegacyRule3001 -ErrorAction SilentlyContinue)
                mesh443    = $meshObj
                exc3001    = $excObj
                tun        = $tun
                bypass     = [ordered]@{
                    node         = $nodeBypass
                    caddy        = $caddyBypass
                    easytierWide = $etWide
                    ddnsGo       = $ddnsBypass
                    nodeSkipped  = $nodeSkipped
                }
            } | ConvertTo-Json -Compress -Depth 4
            exit 0
        }
        'ensure-whitelist' {
            Write-Step (T '443 白名单幂等就位（源 ∈ 组网虚拟网段 + TUN 接口，Profile Any）' 'Ensure the 443 whitelist (source in mesh CIDR + TUN interface, Profile Any)')
            $ok = Invoke-EnsureWhitelist -C $Cidr -Vip $VirtualIp -Wait $WaitTun
            # 程序规则清理（AC11）与 TUN/白名单解耦：TUN 未就绪（休眠）也照常清理
            Invoke-BypassCleanup -Stack $StackDir
            if (-not $ok) { exit 3 }
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
            $ok = Invoke-EnsureWhitelist -C $Cidr -Vip $VirtualIp -Wait $WaitTun
            # 程序规则清理（AC11）与 TUN/白名单解耦：TUN 未就绪（休眠）也照常清理
            Invoke-BypassCleanup -Stack $StackDir
            if (-not $ok) { exit 3 }
        }
    }
} catch {
    Write-Bad "$_"
    exit 1
}
