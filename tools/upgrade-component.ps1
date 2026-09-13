<#
.SYNOPSIS
  组件升级一站式工具（spec 011 T5 / AC7）：caddy / easytier 锁定值的
  下载-校验-改写-测试一条命令。仓库侧开发工具，不随安装包分发。

.DESCRIPTION
  流程（spec 011 §4「升级脚本边界」：先下载校验、后改写，绝不半写锁定值）：
    1. 按钉版形态构造下载 URL，下载到系统临时目录（仓库外）；
    2. 完整性预检（MZ 头 + 最小体积）+ 纯 .NET SHA256 计算；
    3. 全部成功后才改写锁定值——按常量名锚定：
       - caddy    : tools/sprint0/bin/install-https.ps1 头部 $CaddyCoreVersion /
                    $CaddySha256（可选 $CaddyPluginModule）单行常量对；
       - easytier : apps/workbench/src-tauri/src/consts.rs 五个 SHA256 常量
                    （rustfmt 折行——跨行锚定常量名，取其后首个 64-hex 引号串）
                    + resources/bin 五个二进制（仓库内置即源头）；
    4. 有文件改动时自动跑 cargo test（--manifest-path），全绿才算完成；
    5. 末尾输出：新旧值对照 / 改动文件清单 / 建议提交信息（关联 spec011）/
       TOFU 首次信任提示（下载 URL + 新哈希）。

  实现口径（本机坑规避，spec 011 plan §3 升级脚本节）：
    - 哈希一律纯 .NET SHA256——Get-FileHash 在被 PowerShell 7 污染
      PSModulePath 的 5.1 会话里不可用（T4 实测教训，安全闸门不赌环境）；
    - 直连 GitHub / caddyserver.com 可能被网络拦截，下载失败自动改走
      -Proxy 回落代理（默认 http://127.0.0.1:7890，传 '' 禁用回落）；
    - 文本改写保留原编码（BOM 探测）与原行尾（纯正则替换，不重排内容）；
    - 上游构建站只伺服当时最新核心版（旧版本 URL 返回 HTTP 400）——
      下载失败伴随 400 时提示确认版本号是否已被上游淘汰；
    - 明确不同步 resources 下 ps1 副本与 manifest.json（打包时
      scripts/build.ps1 既有同步段负责）。

  幂等：目标版本与现锁定值一致时为无操作（不写文件、不跑测试），
  可用同一版本号重复执行做交叉验证（TOFU 补救：换网络/机器再跑一次，
  比对两次哈希一致后再提交新锁定值）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools\upgrade-component.ps1 -Component caddy -Version v2.11.5
  powershell -NoProfile -ExecutionPolicy Bypass -File tools\upgrade-component.ps1 -Component caddy -Version v2.11.5 -PluginVersion v0.4.4
  powershell -NoProfile -ExecutionPolicy Bypass -File tools\upgrade-component.ps1 -Component easytier -Version v2.6.5
  powershell -NoProfile -ExecutionPolicy Bypass -File tools\upgrade-component.ps1 -Component caddy -Version v2.11.4 -Proxy ''
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('caddy', 'easytier')]
    [string]$Component,

    [Parameter(Mandatory = $true)]
    [string]$Version,

    # 仅 caddy 用：tencentcloud DNS 插件版本；缺省 = install-https.ps1 现值
    [string]$PluginVersion = '',

    # 直连失败时的回落代理；传 ''（空串）禁用回落
    [string]$Proxy = 'http://127.0.0.1:7890'
)

$ErrorActionPreference = 'Stop'

# PS 5.1 默认不启用 TLS 1.2，两个下载源都会握手失败；关进度条可大幅加速
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'

$RepoRoot   = Split-Path -Parent $PSScriptRoot
$InstallHttps = Join-Path $RepoRoot 'tools\sprint0\bin\install-https.ps1'
$ConstsRs     = Join-Path $RepoRoot 'apps\workbench\src-tauri\src\consts.rs'
$ResBin       = Join-Path $RepoRoot 'apps\workbench\src-tauri\resources\bin'
$CargoToml    = Join-Path $RepoRoot 'apps\workbench\src-tauri\Cargo.toml'

# ---------- 输出辅助 ----------
function Write-Step { param([string]$Message) Write-Host "`n==> $Message" -ForegroundColor Cyan }
function Write-Ok   { param([string]$Message) Write-Host "    [OK] $Message"  -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "    [i ] $Message"  -ForegroundColor DarkGray }
function Write-Warn { param([string]$Message) Write-Host "    [!] $Message"   -ForegroundColor Yellow }
function Write-Bad  { param([string]$Message) Write-Host "    [X ] $Message"  -ForegroundColor Red }

# ---------- 纯 .NET SHA256（不用 Get-FileHash：PSModulePath 被 PS7 污染的 5.1 会话不可用） ----------
function Get-FileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $stream = [IO.File]::OpenRead($Path)
        try { return [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '') }
        finally { $stream.Close() }
    } finally { $sha256.Dispose() }
}

# ---------- 编码/行尾原样保留的文本读写 ----------
function Read-TextRaw { param([Parameter(Mandatory = $true)][string]$Path) return [IO.File]::ReadAllText($Path) }
function Write-TextRaw {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Text)
    $bytes = [IO.File]::ReadAllBytes($Path)
    $hasBom = ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)
    $enc = New-Object System.Text.UTF8Encoding($hasBom)
    [IO.File]::WriteAllText($Path, $Text, $enc)
}

# ---------- 下载：直连失败自动回落代理 ----------
function Invoke-DownloadSmart {
    param([Parameter(Mandatory = $true)][string]$Url, [Parameter(Mandatory = $true)][string]$OutFile,
          [int]$TimeoutSec = 1800, [string]$Proxy = 'http://127.0.0.1:7890')
    $common = @{ Uri = $Url; OutFile = $OutFile; TimeoutSec = $TimeoutSec; UseBasicParsing = $true }
    try {
        Invoke-WebRequest @common
        return 'direct'
    } catch {
        $firstErr = $_.Exception.Message
        if ($Proxy) {
            Write-Warn ("直连失败（{0}），改走回落代理 {1} 重试" -f $firstErr, $Proxy)
            Invoke-WebRequest @common -Proxy $Proxy
            return 'proxy'
        }
        throw
    }
}

# ---------- PE 预检：MZ 头 + 最小体积（防把错误页当二进制） ----------
function Test-BinaryArtifact {
    param([Parameter(Mandatory = $true)][string]$Path, [long]$MinBytes, [string]$Label)
    $len = (Get-Item $Path).Length
    if ($len -lt $MinBytes) { throw ("{0} 体积 {1} 字节低于下限 {2}——疑似错误页而非二进制" -f $Label, $len, $MinBytes) }
    $fs = [IO.File]::OpenRead($Path)
    try {
        $head = New-Object byte[] 2
        $null = $fs.Read($head, 0, 2)
        if ([Text.Encoding]::ASCII.GetString($head) -ne 'MZ') { throw ("{0} 缺 MZ 头——不是 Windows PE 产物" -f $Label) }
    } finally { $fs.Close() }
    Write-Ok ("{0} 预检通过（{1} 字节，MZ 头）" -f $Label, $len)
}

# ---------- 版本归一：x.y.z / vX.Y.Z 均可，统一为 vX.Y.Z ----------
function Normalize-VVersion {
    param([Parameter(Mandatory = $true)][string]$V)
    $v = $V.Trim()
    if ($v -notmatch '^v') { $v = "v$v" }
    if ($v -notmatch '^v\d+\.\d+\.\d+$') { throw ("版本号形态应为 x.y.z（收到：{0}）" -f $V) }
    return $v
}

# ---------- install-https.ps1 单行常量锚定（$Name = 'value'） ----------
function Get-Ps1ConstValue {
    param([string]$Text, [Parameter(Mandatory = $true)][string]$Name)
    $m = [regex]::Match($Text, ('[\$]' + $Name + "\s*=\s*'([^']+)'"))
    if (-not $m.Success) { return $null }
    return $m.Groups[1].Value
}
function Set-Ps1ConstValue {
    param([string]$Text, [Parameter(Mandatory = $true)][string]$Name, [Parameter(Mandatory = $true)][string]$NewValue)
    $p = '(?<=[\$]' + $Name + "\s*=\s*)" + "'[^']*'"
    if ([regex]::Matches($Text, $p).Count -ne 1) { throw ('install-https.ps1 中锚点 $' + $Name + ' 匹配数不为 1，拒绝改写') }
    return [regex]::Replace($Text, $p, "'" + $NewValue + "'")
}

# ---------- consts.rs 折行常量锚定（跨行取常量名后首个 64-hex 引号串） ----------
function Get-RustConstHash {
    param([string]$Text, [Parameter(Mandatory = $true)][string]$Name)
    $m = [regex]::Match($Text, ('(?s)' + $Name + '\b[^"]*?"([0-9A-Fa-f]{64})"'))
    if (-not $m.Success) { throw ("consts.rs 未找到常量 {0} 的 64-hex 值" -f $Name) }
    return $m.Groups[1].Value
}
function Set-RustConstHash {
    param([string]$Text, [Parameter(Mandatory = $true)][string]$Name, [Parameter(Mandatory = $true)][string]$NewHash)
    # 变长后行断言（.NET 特性）：[^"]*? 无法跨过任何引号，故必然锚定到常量名后的
    # 首个引号串——rustfmt 把值折到下一行也能命中
    $p = '(?s)(?<=' + $Name + '\b[^"]*?")[0-9A-Fa-f]{64}(?=")'
    if ([regex]::Matches($Text, $p).Count -ne 1) { throw ("consts.rs 中锚点 {0} 匹配数不为 1，拒绝改写" -f $Name) }
    return [regex]::Replace($Text, $p, $NewHash)
}

# =====================================================================
# 主流程
# =====================================================================
$ver = Normalize-VVersion $Version
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('upgrade-component-' + [GUID]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
$changedFiles = New-Object System.Collections.Generic.List[string]
$comparisons  = New-Object System.Collections.Generic.List[string]
$reportUrl    = ''
$reportHashes = New-Object System.Collections.Generic.List[string]

try {
    if ($Component -eq 'caddy') {
        if (-not (Test-Path $InstallHttps)) { throw "找不到 $InstallHttps" }
        # ---- 读取现锁定值 ----
        $ps1Text         = Read-TextRaw $InstallHttps
        $oldCore         = Get-Ps1ConstValue $ps1Text 'CaddyCoreVersion'
        $oldSha          = Get-Ps1ConstValue $ps1Text 'CaddySha256'
        $oldPluginModule = Get-Ps1ConstValue $ps1Text 'CaddyPluginModule'
        if (-not $oldCore -or -not $oldSha -or -not $oldPluginModule) {
            throw 'install-https.ps1 头部常量锚点缺失（$CaddyCoreVersion / $CaddySha256 / $CaddyPluginModule）——锚定改写无从下手'
        }
        if (-not $PluginVersion) {
            if ($oldPluginModule -notmatch '@(v[\d.]+)$') { throw ("无法从 `$CaddyPluginModule 现值解析插件版本：{0}" -f $oldPluginModule) }
            $PluginVersion = $Matches[1]
        } else {
            $PluginVersion = Normalize-VVersion $PluginVersion
        }

        # ---- 钉版 URL（T4 实证：p= 模块@版本 双参数；扁平 version= 参数被服务端无视） ----
        $coreModule   = "github.com/caddyserver/caddy/v2@$ver"
        $pluginModule = "github.com/caddy-dns/tencentcloud@$PluginVersion"
        $url = 'https://caddyserver.com/api/download?os=windows&arch=amd64&p=' +
               [uri]::EscapeDataString($coreModule) + '&p=' + [uri]::EscapeDataString($pluginModule)
        Write-Step ("组件 caddy：下载钉版构建（核心 {0} + 插件 {1}，云端编译需数分钟）" -f $ver, $pluginModule)
        Write-Info "    $url"
        $reportUrl = $url
        $dl = Join-Path $tmp 'caddy-build.exe'
        try {
            $via = Invoke-DownloadSmart -Url $url -OutFile $dl -TimeoutSec 1800 -Proxy $Proxy
        } catch {
            Write-Bad ("下载失败：{0}" -f $_.Exception.Message)
            if (("{0}" -f $_.Exception.Message) -match '\(400\)|Bad Request') {
                Write-Bad '上游构建站只伺服当时最新核心版——旧版本 URL 一律 HTTP 400；请确认版本号是否已被上游淘汰（plan §7 风险表）'
            }
            throw
        }
        Write-Ok ("下载完成（{0} 通道，{1} MB）" -f $via, [math]::Round((Get-Item $dl).Length / 1MB, 1))

        # ---- 预检 + 哈希（此点之前不碰任何仓库文件） ----
        Test-BinaryArtifact -Path $dl -MinBytes 10MB -Label 'caddy 构建产物'
        $newSha = Get-FileSha256 $dl
        $reportHashes.Add(("caddy（{0} + tencentcloud 插件）: {1}" -f $coreModule, $newSha))

        # ---- 校验已过，改写常量对（派生常量 $CaddyCoreModule 由插值自动跟随，无需改） ----
        $newText = $ps1Text
        foreach ($pair in @(
            @{ Name = 'CaddyCoreVersion';  New = $ver;           Old = $oldCore },
            @{ Name = 'CaddySha256';       New = $newSha;        Old = $oldSha },
            @{ Name = 'CaddyPluginModule'; New = $pluginModule;  Old = $oldPluginModule }
        )) {
            $same = ("{0}" -f $pair.Old) -ceq ("{0}" -f $pair.New)
            $comparisons.Add(("caddy {0,-19}: {1} -> {2}{3}" -f $pair.Name, $pair.Old, $pair.New, $(if ($same) { '（未变）' } else { '（更新）' })))
            if (-not $same) { $newText = Set-Ps1ConstValue $newText $pair.Name $pair.New }
        }
        if ($newText -cne $ps1Text) {
            Write-TextRaw $InstallHttps $newText
            $changedFiles.Add('tools/sprint0/bin/install-https.ps1')
        }
    }
    else {
        if (-not (Test-Path $ConstsRs)) { throw "找不到 $ConstsRs" }
        if (-not (Test-Path $ResBin))   { throw "找不到 $ResBin" }

        # ---- 官方 release 包（spec 007 资产命名约定） ----
        $zipName = "easytier-windows-x86_64-$ver.zip"
        $etUrl = "https://github.com/EasyTier/EasyTier/releases/download/$ver/$zipName"
        Write-Step ("组件 easytier：下载官方 release 包（{0}）" -f $etUrl)
        Write-Info "    $etUrl"
        $reportUrl = $etUrl
        $zipPath = Join-Path $tmp $zipName
        try {
            $via = Invoke-DownloadSmart -Url $etUrl -OutFile $zipPath -TimeoutSec 900 -Proxy $Proxy
        } catch {
            Write-Bad ("下载失败：{0}" -f $_.Exception.Message)
            Write-Bad ("请到 https://github.com/EasyTier/EasyTier/releases 核对 tag {0} 的资产命名（脚本按 {1} 约定）" -f $ver, $zipName)
            throw
        }
        Write-Ok ("下载完成（{0} 通道，{1} MB）" -f $via, [math]::Round((Get-Item $zipPath).Length / 1MB, 1))

        $extractDir = Join-Path $tmp 'x'
        Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

        $map = @(
            @{ File = 'easytier-core.exe'; Const = 'EASYTIER_CORE_SHA256' },
            @{ File = 'easytier-cli.exe';  Const = 'EASYTIER_CLI_SHA256' },
            @{ File = 'wintun.dll';        Const = 'WINTUN_DLL_SHA256' },
            @{ File = 'Packet.dll';        Const = 'PACKET_DLL_SHA256' },
            @{ File = 'WinDivert64.sys';   Const = 'WINDIVERT_SYS_SHA256' }
        )

        # ---- 阶段一（只读）：定位 + 预检 + 哈希 + 取现锁定值 ----
        $verified = @()
        foreach ($item in $map) {
            $found = Get-ChildItem -Path $extractDir -Recurse -Filter $item.File | Select-Object -First 1
            if (-not $found) { throw ("release 包内缺少 {0}——资产内容与预期不符，中止（未改写任何文件）" -f $item.File) }
            Test-BinaryArtifact -Path $found.FullName -MinBytes 64KB -Label $item.File
            $hash = Get-FileSha256 $found.FullName
            $oldConst = Get-RustConstHash (Read-TextRaw $ConstsRs) $item.Const
            $verified += @{ File = $item.File; Const = $item.Const; New = $hash; Old = $oldConst; Src = $found.FullName }
            $reportHashes.Add(("{0}: {1}" -f $item.File, $hash.ToLower()))
        }

        # ---- 阶段二（校验已过）：复制二进制 + 改写 consts.rs（均只在与现值不同时发生） ----
        $rustText = Read-TextRaw $ConstsRs
        $rustChanged = $false
        foreach ($v in $verified) {
            $dst = Join-Path $ResBin $v.File
            $dstSame = (Test-Path $dst) -and ((Get-FileSha256 $dst) -ceq $v.New)
            if ($dstSame) {
                Write-Ok ("{0} 与 resources/bin 现有文件字节一致，跳过复制" -f $v.File)
            } else {
                Copy-Item -Path $v.Src -Destination $dst -Force
                $changedFiles.Add("apps/workbench/src-tauri/resources/bin/$($v.File)")
                Write-Ok ("{0} -> {1}" -f $v.File, $dst)
            }
            $same = ("{0}" -f $v.Old) -ieq ("{0}" -f $v.New)
            $comparisons.Add(("{0,-19}: {1} -> {2}{3}" -f $v.Const, $v.Old, $v.New.ToLower(), $(if ($same) { '（未变）' } else { '（更新）' })))
            if (-not $same) {
                $rustText = Set-RustConstHash $rustText $v.Const $v.New.ToLower()
                $rustChanged = $true
            }
        }
        if ($rustChanged) {
            Write-TextRaw $ConstsRs $rustText
            $changedFiles.Add('apps/workbench/src-tauri/src/consts.rs')
        }
    }

    # ---- 测试联动（无文件改动则无代码可测，跳过） ----
    if ($changedFiles.Count -gt 0) {
        Write-Step '改动已落盘，运行 cargo test（全绿才算完成）'
        try {
            & cargo test --manifest-path $CargoToml
        } catch {
            Write-Bad ("无法启动 cargo：{0}" -f $_.Exception.Message)
            exit 1
        }
        if ($LASTEXITCODE -ne 0) {
            Write-Bad ("cargo test 失败（退出码 {0}）。锁定值已按已验证的下载产物改写，但测试未过——可 git checkout -- 还原后排查" -f $LASTEXITCODE)
            exit 1
        }
        Write-Ok 'cargo test 全绿'
    } else {
        Write-Step '锁定值与内置二进制均与下载产物一致——无文件改动（无操作），跳过 cargo test'
    }

    # ---- 汇总输出 ----
    Write-Host ''
    Write-Host '==================== 新旧值对照 ====================' -ForegroundColor Magenta
    foreach ($line in $comparisons) { Write-Host "  $line" }
    Write-Host ''
    Write-Host '==================== 改动文件清单 ====================' -ForegroundColor Magenta
    if ($changedFiles.Count -eq 0) {
        Write-Host '  （无——当前锁定值已与下载产物一致，本次为无操作）'
    } else {
        foreach ($f in $changedFiles) { Write-Host "  $f" }
    }
    Write-Host ''
    Write-Host '==================== 建议提交信息（关联 spec011） ====================' -ForegroundColor Magenta
    if ($Component -eq 'caddy') {
        Write-Host ("  feat(spec011): 升级 caddy 锁定值 {0} -> {1}（tools/upgrade-component.ps1 自动改写）" -f $oldCore, $ver)
    } else {
        Write-Host ("  feat(spec011): 升级 easytier -> {0}（consts.rs 五哈希 + resources/bin 五文件，tools/upgrade-component.ps1 改写）" -f $ver)
    }
    Write-Host ''
    Write-Host '==================== TOFU 首次信任提示 ====================' -ForegroundColor Magenta
    Write-Host "  下载 URL：$reportUrl"
    foreach ($h in $reportHashes) { Write-Host "  新锁定哈希：$h" }
    if ($Component -eq 'caddy') {
        Write-Host '  caddy 官方 stock 构建的 GitHub release 校验和（https://github.com/caddyserver/caddy/releases'
        Write-Host '  的 checksums）仅覆盖无插件构建，只能作核心版本参考；本产物含 tencentcloud 插件、'
        Write-Host '  属定制构建无上游校验和——首次锁定值以本次下载自证（TOFU）。'
    } else {
        Write-Host '  easytier 哈希取自官方 release 包内件；首次锁定值以本次下载自证（TOFU），'
        Write-Host '  可到 https://github.com/EasyTier/EasyTier/releases 人工核对 tag 与资产。'
    }
    Write-Host '  如需更强保障：换网络/机器以同一版本号重跑本脚本（无操作模式），比对两次哈希一致后再提交。'
    Write-Host '=====================================================' -ForegroundColor Magenta
}
finally {
    Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
