<#
.SYNOPSIS
  旧通道一次性卸载（spec 008）：清除 frp 穿透与 ddns-go 直连在本机的全部残留。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  spec 007 已停用 frp / ddns-go 通道（保留 EasyTier 组网、Caddy 域名与局域网 3001），
  本脚本做存量收尾清理，幂等、可重复执行：
    1. 前置闸门：EasyTierMesh 服务必须存在且 Running——卸掉旧通道前组网是唯一的
       远程访问兜底（-Force 可跳过，仅供演练）
    2. 凭证闸门：栈 .env 缺 TENCENT_SECRET_ID / TENCENT_SECRET_KEY 任一项且
       ddns-go.yaml 仍存在 → 该 yaml 拒绝删除（它是 DNSPod 记录在本机的唯一描述，
       无密钥时工作台无法接管清理）。先跑 set-tencent-key.ps1 交互输入密钥再重跑
    3. 停止 frpc / ddns-go 进程（优先匹配可执行路径位于栈目录下的实例；路径读不到
       的同名实例按名一并停止；明确位于栈目录之外的不触碰）
    4. 注销计划任务 "ddns-go Sprint0 autostart"（与 setup-autostart.ps1 定义精确一致）
    5. 删除 ddns-go.exe / ddns-go.yaml / frpc.exe / frpc-run.log
    6. 从栈 .env 移除 SAKURA_FRP_KEY 行（其余行与 UTF-8 BOM 形态原样保留）
    7. CNAME 残留检测：ai.jackqi.cn 仍解析出 CNAME → 提示去工作台设置页执行
       「同步 DNS」清理（本脚本不直接调 DNSPod API）；查询失败按「无法判断」提示人工核验

  ★ 真机执行前置闸门：spec 007 验收签收之后才可执行（spec 008 tasks 约定）。
  无需管理员权限（栈目录、.env、当前用户的进程与计划任务均可直接操作）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall-legacy.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall-legacy.ps1 -Force   # 演练：跳过组网在线校验
#>
[CmdletBinding()]
param(
    [string]$StackDir = 'D:\Software\cloudcli-https',
    [switch]$Force,
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
)

$ErrorActionPreference = 'Stop'

# ---------- 语言 / 输出辅助（sprint0 约定：T() + Write-Ok/Info/Warn/Bad） ----------
if ($Lang -eq 'auto') {
    $script:lang = if ((Get-UICulture).TwoLetterISOLanguageName -eq 'zh') { 'zh' } else { 'en' }
} else {
    $script:lang = $Lang
}
function T { param([string]$zh, [string]$en) if ($script:lang -eq 'zh') { $zh } else { $en } }
function Write-Ok   { param([string]$Message) Write-Host "[OK] $Message" -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "[i ] $Message"  -ForegroundColor DarkGray }
function Write-Warn { param([string]$Message) Write-Host "[!] $Message"   -ForegroundColor Yellow }
function Write-Bad  { param([string]$Message) Write-Host "[X ] $Message"  -ForegroundColor Red }

# ---------- 汇总账本（每步 Done / Skipped / Failed，决定最终退出码） ----------
$script:ledger = [System.Collections.Generic.List[object]]::new()
function Record {
    param([string]$Step, [ValidateSet('Done', 'Skipped', 'Failed')][string]$Status, [string]$Note = '')
    $script:ledger.Add([pscustomobject]@{ Step = $Step; Status = $Status; Note = $Note })
}

# ---------- 路径常量 ----------
$EnvFile  = Join-Path $StackDir '.env'
$YamlPath = Join-Path $StackDir 'ddns-go.yaml'
$TaskName = 'ddns-go Sprint0 autostart'

# ---------- 0. 前置闸门：组网必须在线（-Force 跳过，仅供演练） ----------
if (-not $Force) {
    $mesh = Get-Service -Name 'EasyTierMesh' -ErrorAction SilentlyContinue
    if (-not $mesh) {
        Write-Bad (T '未找到服务 EasyTierMesh：卸载旧通道前组网必须在线——这是唯一的远程访问兜底。请先用 mesh-service.ps1 启用组网再重跑本脚本；仅演练可用 -Force 跳过。' 'Service EasyTierMesh not found: the mesh must be online before removing the legacy channels - it is the only remote-access fallback. Enable the mesh first (mesh-service.ps1), then re-run; use -Force to skip for dry runs only.')
        exit 1
    }
    if ($mesh.Status -ne 'Running') {
        Write-Bad (T "服务 EasyTierMesh 当前状态为 $($mesh.Status)：卸载旧通道前组网必须在线——这是唯一的远程访问兜底。请先启动组网再重跑本脚本；仅演练可用 -Force 跳过。" "Service EasyTierMesh is '$($mesh.Status)': the mesh must be online before removing the legacy channels - it is the only remote-access fallback. Start the mesh first, then re-run; use -Force to skip for dry runs only.")
        exit 1
    }
    Write-Ok (T '组网服务在线：EasyTierMesh（远程访问兜底就绪）。' 'Mesh service online: EasyTierMesh (remote-access fallback ready).')
} else {
    Write-Warn (T '-Force：已跳过组网在线校验（仅供演练）。' '-Force: mesh online check skipped (dry run only).')
}

# ---------- 1. 凭证闸门：缺腾讯云密钥时拒绝删除 ddns-go.yaml ----------
$mayDeleteYaml = $true
if (Test-Path $YamlPath) {
    $secretId = ''; $secretKey = ''
    if (Test-Path $EnvFile) {
        # ReadAllLines 自动兼容并剥离 UTF-8 BOM
        foreach ($line in [System.IO.File]::ReadAllLines($EnvFile)) {
            if ($line -match '^\s*TENCENT_SECRET_ID\s*=\s*(.*)$')  { $secretId   = $Matches[1].Trim() }
            if ($line -match '^\s*TENCENT_SECRET_KEY\s*=\s*(.*)$') { $secretKey = $Matches[1].Trim() }
        }
    }
    if ([string]::IsNullOrWhiteSpace($secretId) -or [string]::IsNullOrWhiteSpace($secretKey)) {
        $mayDeleteYaml = $false
        Write-Warn (T "检测到 $YamlPath，但栈 .env 缺腾讯云密钥（TENCENT_SECRET_ID / TENCENT_SECRET_KEY）——该 yaml 拒绝删除。" "$YamlPath found, but the stack .env lacks the Tencent Cloud key (TENCENT_SECRET_ID / TENCENT_SECRET_KEY) - the yaml will NOT be deleted.")
        Write-Host (T '  它是 DNSPod 记录在本机的唯一描述，没有密钥工作台就无法接管清理。先交互输入腾讯云密钥，再重跑本脚本：' '  It is the only local description of the DNSPod records; without the key the workbench cannot take over the cleanup. Store the Tencent Cloud key interactively first, then re-run this script:') -ForegroundColor Yellow
        Write-Host '    powershell -ExecutionPolicy Bypass -File .\set-tencent-key.ps1' -ForegroundColor Yellow
    }
}

# ---------- 2. 停止 frpc / ddns-go 进程 ----------
foreach ($procName in @('frpc', 'ddns-go')) {
    $stepName = (T "停止进程 $procName" "Stop process $procName")
    $procs = @(Get-Process -Name $procName -ErrorAction SilentlyContinue)
    if ($procs.Count -eq 0) {
        Record -Step $stepName -Status Skipped -Note (T '未在运行' 'not running')
        Write-Info (T "$procName 未在运行，跳过。" "$procName not running, skipped.")
        continue
    }
    # 优先栈目录实例；路径读不到（空/拒绝访问）的同名实例按名一并停止；明确在栈外的不碰
    $targets = @()
    foreach ($p in $procs) {
        $pPath = $null
        try { $pPath = $p.Path } catch { }
        if ([string]::IsNullOrEmpty($pPath) -or $pPath.StartsWith($StackDir, [System.StringComparison]::OrdinalIgnoreCase)) {
            $targets += $p
        }
    }
    if ($targets.Count -eq 0) {
        Record -Step $stepName -Status Skipped -Note (T '实例均在栈目录之外，未触碰' 'all instances live outside the stack dir; untouched')
        Write-Info (T "$procName 的实例均在栈目录之外，不触碰。" "All $procName instances live outside the stack dir; untouched.")
        continue
    }
    $outside = $procs.Count - $targets.Count
    try {
        foreach ($t in $targets) { Stop-Process -Id $t.Id -Force -ErrorAction Stop }
        foreach ($t in $targets) { Wait-Process -Id $t.Id -Timeout 15 -ErrorAction SilentlyContinue }
        $still = @($targets | Where-Object { Get-Process -Id $_.Id -ErrorAction SilentlyContinue })
        if ($still.Count -gt 0) {
            Record -Step $stepName -Status Failed -Note (T "$($still.Count) 个实例 15 秒内未退出" "$($still.Count) instance(s) did not exit within 15 s")
            Write-Bad (T "$procName 有实例 15 秒内未退出。" "Some $procName instance(s) did not exit within 15 s.")
        } else {
            $note = (T "已停止 $($targets.Count) 个实例" "Stopped $($targets.Count) instance(s)")
            if ($outside -gt 0) { $note += (T "；栈外 $outside 个未触碰" "; $outside outside the stack dir untouched") }
            Record -Step $stepName -Status Done -Note $note
            Write-Ok $note
        }
    } catch {
        Record -Step $stepName -Status Failed -Note $_.Exception.Message
        Write-Bad (T "停止 $procName 失败：$($_.Exception.Message)" "Failed to stop $procName : $($_.Exception.Message)")
    }
}

# ---------- 3. 注销计划任务 ----------
$stepName = (T "注销计划任务 $TaskName" "Unregister scheduled task $TaskName")
try {
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction Stop
        Record -Step $stepName -Status Done
        Write-Ok (T '已注销。' 'Unregistered.')
    } else {
        Record -Step $stepName -Status Skipped -Note (T '不存在' 'not present')
        Write-Info (T '任务不存在，跳过。' 'Task not present, skipped.')
    }
} catch {
    Record -Step $stepName -Status Failed -Note $_.Exception.Message
    Write-Bad (T "注销失败：$($_.Exception.Message)" "Unregister failed: $($_.Exception.Message)")
}

# ---------- 4. 删除文件（存在才删，逐项报告；yaml 受凭证闸门约束） ----------
foreach ($fileName in @('ddns-go.exe', 'frpc.exe', 'frpc-run.log')) {
    $path = Join-Path $StackDir $fileName
    $stepName = (T "删除 $fileName" "Delete $fileName")
    if (-not (Test-Path $path)) {
        Record -Step $stepName -Status Skipped -Note (T '不存在' 'not present')
        Write-Info (T "$fileName 不存在，跳过。" "$fileName not present, skipped.")
        continue
    }
    try {
        Remove-Item -Path $path -Force -ErrorAction Stop
        if (Test-Path $path) { throw (T '删除后复核仍存在' 'still present after deletion') }
        Record -Step $stepName -Status Done
        Write-Ok (T "已删除：$path" "Deleted: $path")
    } catch {
        Record -Step $stepName -Status Failed -Note $_.Exception.Message
        Write-Bad (T "删除 $fileName 失败：$($_.Exception.Message)" "Failed to delete $fileName : $($_.Exception.Message)")
    }
}
$stepName = (T '删除 ddns-go.yaml' 'Delete ddns-go.yaml')
if (-not (Test-Path $YamlPath)) {
    Record -Step $stepName -Status Skipped -Note (T '不存在' 'not present')
    Write-Info (T 'ddns-go.yaml 不存在，跳过。' 'ddns-go.yaml not present, skipped.')
} elseif (-not $mayDeleteYaml) {
    Record -Step $stepName -Status Skipped -Note (T '凭证闸门：缺腾讯云密钥，拒绝删除（见上方指引）' 'credential gate: Tencent Cloud key missing, deletion refused (see guidance above)')
    Write-Warn (T 'ddns-go.yaml 保留（缺腾讯云密钥，见上方指引）。' 'ddns-go.yaml kept (Tencent Cloud key missing; see guidance above).')
} else {
    try {
        Remove-Item -Path $YamlPath -Force -ErrorAction Stop
        if (Test-Path $YamlPath) { throw (T '删除后复核仍存在' 'still present after deletion') }
        Record -Step $stepName -Status Done
        Write-Ok (T "已删除：$YamlPath" "Deleted: $YamlPath")
    } catch {
        Record -Step $stepName -Status Failed -Note $_.Exception.Message
        Write-Bad (T "删除 ddns-go.yaml 失败：$($_.Exception.Message)" "Failed to delete ddns-go.yaml : $($_.Exception.Message)")
    }
}

# ---------- 5. .env 清除 SAKURA_FRP_KEY 行（其余行与 BOM 形态原样保留） ----------
$stepName = (T '.env 清除 SAKURA_FRP_KEY 行' '.env: remove the SAKURA_FRP_KEY line')
if (-not (Test-Path $EnvFile)) {
    Record -Step $stepName -Status Skipped -Note (T '无 .env' 'no .env')
    Write-Info (T "未找到 .env（$EnvFile），无可清理。" "No .env found ($EnvFile); nothing to clean.")
} else {
    $lines = [System.IO.File]::ReadAllLines($EnvFile)
    $kept  = @($lines | Where-Object { $_ -notmatch '^\s*SAKURA_FRP_KEY\s*=' })
    if ($kept.Count -eq $lines.Count) {
        Record -Step $stepName -Status Skipped -Note (T '无该行，无可清理' 'line absent; nothing to clean')
        Write-Info (T '.env 中未发现 SAKURA_FRP_KEY 行，无可清理。' 'No SAKURA_FRP_KEY line in .env; nothing to clean.')
    } else {
        try {
            # UTF-8 BOM：与 set-tencent-key.ps1 / clear-frp-key.ps1 的写入形态一致
            $utf8Bom = [System.Text.UTF8Encoding]::new($true)
            [System.IO.File]::WriteAllLines($EnvFile, $kept, $utf8Bom)
            if (Select-String -Path $EnvFile -Pattern '^\s*SAKURA_FRP_KEY\s*=' -Quiet) { throw (T '复核仍存在该行' 'line still present on re-check') }
            Record -Step $stepName -Status Done -Note (T "移除 $($lines.Count - $kept.Count) 行，其余行原样保留" "removed $($lines.Count - $kept.Count) line(s); other lines kept")
            Write-Ok (T "已从 .env 移除 SAKURA_FRP_KEY 行（其余行原样保留）：$EnvFile" "SAKURA_FRP_KEY line removed from .env (other lines kept): $EnvFile")
        } catch {
            Record -Step $stepName -Status Failed -Note $_.Exception.Message
            Write-Bad (T ".env 清理失败：$($_.Exception.Message)" ".env cleanup failed: $($_.Exception.Message)")
        }
    }
}

# ---------- 6. CNAME 残留检测（只提示，不直接调 DNSPod API） ----------
$stepName = (T 'CNAME 残留检测（ai.jackqi.cn）' 'CNAME residue check (ai.jackqi.cn)')
try {
    $cname = @(Resolve-DnsName -Name 'ai.jackqi.cn' -Type CNAME -ErrorAction Stop | Where-Object { $_.QueryType -eq 'CNAME' })
    if ($cname.Count -gt 0) {
        $target = ($cname | Select-Object -First 1).NameHost
        Record -Step $stepName -Status Done -Note (T "仍指向 $target" "still points to $target")
        Write-Warn (T "发现 CNAME 残留：ai.jackqi.cn -> $target。请在工作台设置页执行「同步 DNS」清理（本脚本不直接调 DNSPod API）。" "CNAME residue found: ai.jackqi.cn -> $target. Run 'Sync DNS' on the workbench settings page to clean it (this script does not call the DNSPod API).")
    } else {
        Record -Step $stepName -Status Done -Note (T '无 CNAME 记录' 'no CNAME record')
        Write-Ok (T 'ai.jackqi.cn 无 CNAME 残留。' 'No CNAME residue on ai.jackqi.cn.')
    }
} catch {
    Record -Step $stepName -Status Skipped -Note (T '查询失败，无法判断' 'query failed; undetermined')
    Write-Warn (T "CNAME 查询失败（$($_.Exception.Message)），无法判断是否残留，请人工核验：nslookup -type=CNAME ai.jackqi.cn" "CNAME query failed ($($_.Exception.Message)); cannot determine residue - verify manually: nslookup -type=CNAME ai.jackqi.cn")
}

# ---------- 7. 汇总 ----------
Write-Host ''
Write-Host (T '==================== 旧通道卸载汇总 ====================' '==================== Legacy channel uninstall summary ====================') -ForegroundColor Magenta
foreach ($r in $script:ledger) {
    $color = switch ($r.Status) { 'Done' { 'Green' } 'Skipped' { 'DarkGray' } 'Failed' { 'Red' } }
    $line = '  [{0,-7}] {1}' -f $r.Status, $r.Step
    if ($r.Note) { $line += " -- $($r.Note)" }
    Write-Host $line -ForegroundColor $color
}
$failedCount = @($script:ledger | Where-Object { $_.Status -eq 'Failed' }).Count
Write-Host ''
if ($failedCount -gt 0) {
    Write-Bad (T "有 $failedCount 步失败，请按上方 [Failed] 行处理后重跑本脚本（幂等）。" "$failedCount step(s) failed; fix the [Failed] lines above and re-run this script (idempotent).")
    exit 1
}
Write-Ok (T '全部步骤 Done / Skipped，无失败。' 'All steps Done / Skipped, no failures.')
exit 0
