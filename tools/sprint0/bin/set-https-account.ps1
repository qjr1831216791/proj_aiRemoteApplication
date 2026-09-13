<#
.SYNOPSIS
  HTTPS 访问账号管理（spec 011 P3）：add/set/remove 三动作——密码只存在于本窗口
  的 Read-Host 与 stdin 管道，绝不进工作台/命令行/日志/settings（开发宪法 §3）。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  443 入口 basic_auth 多账号的**实际执行者**：工作台「访问账号」界面只发起与
  展示（传递的均为非敏感参数：动作/用户名/栈目录，Rust 侧经白名单校验与值
  包裹后派发）；本脚本亦可手动运行。
  事实来源：栈目录 auth-accounts.json —— {"accounts":[{"username":"...","hash":"$2a$..."}]}
  （仅 bcrypt 哈希，无明文）。

  流程（add / set）：
    1. Read-Host -AsSecureString 输密码两次（隐藏回显）→ 内存解包校验：
       非空、≥8 字符、两次一致（中文报错，不符即退出不落盘）
    2. 明文经 stdin 管道喂栈目录 caddy.exe hash-password --algorithm bcrypt
       （密码绝不进命令行；显式算法防上游默认值漂移）→ bcrypt 哈希
    3. 写回 auth-accounts.json → Caddyfile 标记段再生
  remove：无密码交互，直接从 JSON 移除并再生标记段；用户名不存在则报错退出。

  Caddyfile 标记段：# BEGIN workbench-auth / # END workbench-auth 两标记之间
  的内容由 JSON 再生为 basic_auth { <u1> <h1> <u2> <h2> ... }；账号列表为空
  → 标记间只留注释行（空段合法，caddy 正常启动与 validate 通过）。
  标记缺失（v0.6.0 及更早的存量装机）→ 解析出 site 块地址行
  （"<域名>:443 {" 形态）在块首插入标记对。

  安全收尾顺序（硬性契约）：
    写前备份 .bak（Caddyfile 与 auth-accounts.json 同样待遇，保持两文件一致）
    → 写入 → caddy validate --config 自检 → 失败自动回滚 .bak 并报错退出
    → 成功则尝试 caddy reload（admin API 本地回环）；reload 前先确认确有
    以本栈 Caddyfile 为 --config 的 caddy.exe 进程在运行（caddy reload 固定打
    localhost:2019，与本脚本管理哪个栈无关——无守卫会把同机别栈的 caddy
    热加载成本栈配置）；无匹配进程 → 提示「已保存，下次启动 caddy 时生效」（非错误）。

  无需管理员权限（栈目录为当前用户可写）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\set-https-account.ps1 -Action add -Username jack
  powershell -NoProfile -ExecutionPolicy Bypass -File .\set-https-account.ps1 -Action remove -Username jack
#>
[CmdletBinding()]
param(
    [string]$StackDir = 'D:\Software\cloudcli-https',
    [Parameter(Mandatory = $true)]
    [ValidateSet('add', 'set', 'remove')]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [string]$Username,
    [ValidateSet('auto', 'zh', 'en')]
    [string]$Lang = 'auto'
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

# SecureString → 明文（仅存于本函数返回值与内存变量，不落任何输出/日志）
function ConvertTo-PlainText {
    param([Security.SecureString]$Secure)
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Secure)
    try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

# ---------- 路径与前置检查 ----------
$caddy     = Join-Path $StackDir 'caddy.exe'          # caddy 落位于栈目录根（install-https.ps1 契约）
$caddyfile = Join-Path $StackDir 'Caddyfile'
$authFile  = Join-Path $StackDir 'auth-accounts.json'

# 用户名白名单（与工作台 Rust 侧同规格：注入防线第二道——手动运行同样受控）
if ($Username -notmatch '^[a-zA-Z0-9_-]{1,32}$') {
    Write-Bad (T "用户名非法「$Username」：仅允许英文字母/数字/下划线/连字符，长度 1~32。" "Invalid username '$Username': only letters/digits/underscore/hyphen, 1-32 chars.")
    exit 1
}
foreach ($pair in @(
    @($caddy,     (T '栈目录缺少 caddy.exe（哈希生成与配置自检都依赖它）：{0}' 'caddy.exe missing in the stack dir (needed for hashing and validation): {0}')),
    @($caddyfile, (T '栈目录缺少 Caddyfile：{0}' 'Caddyfile missing in the stack dir: {0}'))
)) {
    if (-not (Test-Path $pair[0])) {
        Write-Bad ($pair[1] -f $pair[0])
        exit 1
    }
}

# ---------- 读取既有账号（事实来源：auth-accounts.json） ----------
# PS 5.1 坑：单元素数组的 .accounts 会被解包为单个对象 → 一律 @() 复包裹
$accounts = @()
if (Test-Path $authFile) {
    try {
        $json = Get-Content -Path $authFile -Raw -Encoding UTF8 | ConvertFrom-Json
        $accounts = @($json.accounts)
    } catch {
        Write-Bad (T "访问账号文件损坏（$authFile）：$($_.Exception.Message)——请手动修正或删除该文件后重新添加账号。" "The account file is corrupt ($authFile): $($_.Exception.Message) - fix or delete it manually, then re-add accounts.")
        exit 1
    }
}
$exists = [bool]($accounts | Where-Object { $_.username -ceq $Username })

# ---------- 动作分支 ----------
if ($Action -eq 'add' -and $exists) {
    Write-Bad (T "账号「$Username」已存在：新增请换用户名，修改密码请用 -Action set。" "Account '$Username' already exists: pick another name to add, or use -Action set to change its password.")
    exit 1
}
if ($Action -ne 'add' -and -not $exists) {
    Write-Bad (T "账号「$Username」不存在：无法 $Action。" "Account '$Username' does not exist: cannot $Action.")
    exit 1
}

$newHash = $null
if ($Action -ne 'remove') {
    # 密码输入：只在脚本窗口（隐藏回显两次）——工作台界面无任何密码框（AC11）
    Write-Info (T "为账号「$Username」设置密码（输入不回显；仅字母数字符号即可，至少 8 个字符）。" "Set the password for '$Username' (input hidden; at least 8 characters).")
    $sec1 = Read-Host -AsSecureString (T '密码（输入不回显）' 'Password (input hidden)')
    $sec2 = Read-Host -AsSecureString (T '再输一次确认' 'Confirm the password')
    $pw1 = ConvertTo-PlainText $sec1
    $pw2 = ConvertTo-PlainText $sec2
    if ([string]::IsNullOrWhiteSpace($pw1)) {
        Write-Bad (T '密码为空，未写入。' 'Empty password; nothing written.')
        exit 1
    }
    if ($pw1.Length -lt 8) {
        Write-Bad (T "密码太短（$($pw1.Length) 个字符）：至少需要 8 个字符，未写入。" "Password too short ($($pw1.Length) chars): at least 8 required; nothing written.")
        exit 1
    }
    if ($pw1 -cne $pw2) {
        Write-Bad (T '两次输入不一致，未写入。' 'The two entries do not match; nothing written.')
        exit 1
    }
    if ($pw1 -match '[^\x20-\x7E]') {
        # 真机实证（2026-09-13）：非 ASCII 密码时浏览器对 basic_auth 凭证的编码不一致
        # （Latin-1 vs UTF-8），会出现「密码正确也反复 401」——只警告不拒绝
        Write-Info (T '提示：密码含非 ASCII 字符（如中文/全角符号）——部分浏览器认证编码不一致会导致「密码正确也反复询问」，强烈建议改用纯英文数字符号密码。' 'Note: the password contains non-ASCII characters; inconsistent browser encoding can cause endless 401 prompts even with the correct password. ASCII-only passwords are strongly recommended.')
    }

    # bcrypt 哈希：栈目录 caddy.exe hash-password，明文经 stdin 管道（UTF-8 + 单个 \n 结尾）
    # ——绝不进命令行；字节级控制 stdin 是为避开 PowerShell 管道自带 \r\n 的污染
    # （caddy 按行读到 \n 为止，多出的 \r 会混进哈希输入导致浏览器认证永远失败）
    Write-Info (T '生成 bcrypt 哈希（caddy hash-password，密码经 stdin 管道，不进命令行）...' 'Generating the bcrypt hash (caddy hash-password via stdin; never on the command line)...')
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName               = $caddy
    $psi.Arguments              = 'hash-password --algorithm bcrypt'
    $psi.UseShellExecute        = $false
    $psi.RedirectStandardInput  = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError  = $true
    $proc = [System.Diagnostics.Process]::Start($psi)
    $pwBytes = [Text.Encoding]::UTF8.GetBytes($pw1)
    $proc.StandardInput.BaseStream.Write($pwBytes, 0, $pwBytes.Length)
    $proc.StandardInput.BaseStream.WriteByte(10)   # caddy 读到行尾才返回
    $proc.StandardInput.BaseStream.Flush()
    $proc.StandardInput.Close()
    $newHash = $proc.StandardOutput.ReadToEnd().Trim()
    $hashErr = $proc.StandardError.ReadToEnd().Trim()
    $proc.WaitForExit()
    if ($proc.ExitCode -ne 0 -or $newHash -notmatch '^\$2[aby]\$') {
        Write-Bad (T "哈希生成失败（退出码 $($proc.ExitCode)）：$hashErr" "Hash generation failed (exit $($proc.ExitCode)): $hashErr")
        exit 1
    }
}

# 计算新的账号集合（保持既有顺序；add 追加到末尾）
if ($Action -eq 'add') {
    $accounts += [pscustomobject]@{ username = $Username; hash = $newHash }
} elseif ($Action -eq 'set') {
    foreach ($a in $accounts) {
        if ($a.username -ceq $Username) { $a.hash = $newHash }
    }
} else {
    $accounts = @($accounts | Where-Object { $_.username -cne $Username })
}

# ---------- Caddyfile 标记段再生 ----------
$beginTag = '# BEGIN workbench-auth'
$endTag   = '# END workbench-auth'

# 由 JSON 再生的段内容（ASCII：与 install-https.ps1 生成端 -Encoding ascii 一致；
# 制表符缩进与 caddy fmt 规范化形态一致，避免每次 validate/adapt 的 not formatted 警告）
$segment = @("`t$beginTag")
if ($accounts.Count -eq 0) {
    $segment += "`t#   (no access accounts configured - add via the workbench or set-https-account.ps1)"
} else {
    # Bearer 令牌直通匹配器（2026-09-13 真机实证）：CloudCLI 前端登录后，
    # 其 API 调用把 Authorization 头整个换成 Bearer <token>——若不加匹配器，
    # caddy 拿 Bearer 当 basic 凭证校验必败，回 401 + WWW-Authenticate，
    # 浏览器（尤其移动端/PWA）反复弹原生账密框、永无宁日。带 Bearer 的请求
    # 跳过本门，交由 CloudCLI 自身 token 认证把关（注册端点已实证关闭 403）；
    # 无头/ basic 请求照常校验，纵深不丢。
    # 语法坑（2026-09-13 实证）：PS 5.1 单引号字符串若以致闭引号的反引号收尾，
    # 反引号会被静默吞掉（`^...` 写法曾丢收尾反引号导致 caddy 解析失败）——
    # 一律用双引号 + `` 转义
    $segment += "`t@noBearer not header_regexp Authorization ``^Bearer\s``"
    $segment += "`tbasic_auth @noBearer {"
    foreach ($a in $accounts) {
        $segment += ("`t`t{0} {1}" -f $a.username, $a.hash)
    }
    $segment += "`t}"
}
$segment += "`t$endTag"

$lines = @(Get-Content -Path $caddyfile)
$beginIdx = -1
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^\s*#\s*BEGIN workbench-auth\s*$') { $beginIdx = $i; break }
}

if ($beginIdx -ge 0) {
    # 既有标记段：找到 END（缺失 = 文件被改坏，拒绝盲写）
    $endIdx = -1
    for ($i = $beginIdx + 1; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match '^\s*#\s*END workbench-auth\s*$') { $endIdx = $i; break }
    }
    if ($endIdx -lt 0) {
        Write-Bad (T "Caddyfile 有 BEGIN 无 END 标记（$caddyfile）：请手动补齐后再试，脚本拒绝盲写。" "The Caddyfile has a BEGIN marker without END ($caddyfile): fix it manually first; the script refuses to write blindly.")
        exit 1
    }
    $newLines = @()
    if ($beginIdx -gt 0) { $newLines += $lines[0..($beginIdx - 1)] }
    $newLines += $segment
    if ($endIdx -lt ($lines.Count - 1)) { $newLines += $lines[($endIdx + 1)..($lines.Count - 1)] }
    $lines = $newLines
    Write-Info (T '已定位既有标记段，将整体替换再生。' 'Existing marker section found; it will be replaced wholesale.')
} else {
    # 存量装机（无标记段）：解析 site 块地址行（"<域名>:443 {"），块首插入标记对
    $siteIdx = -1
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match '^\s*[^\s#{][^\s{]*:443\s*\{') { $siteIdx = $i; break }
    }
    if ($siteIdx -lt 0) {
        Write-Bad (T "Caddyfile 中未找到 site 块地址行（应为「域名:443 {」形态）：$caddyfile——请确认文件为插件式 HTTPS 栈所生成。" "No site block address line (expected 'domain:443 {') found in the Caddyfile: $caddyfile - make sure it was generated by the HTTPS stack installer.")
        exit 1
    }
    $newLines = @()
    if ($siteIdx -ge 0) { $newLines += $lines[0..$siteIdx] }
    $newLines += $segment
    if ($siteIdx -lt ($lines.Count - 1)) { $newLines += $lines[($siteIdx + 1)..($lines.Count - 1)] }
    $lines = $newLines
    Write-Info (T '检测到存量 Caddyfile 无标记段：已在 site 块首插入标记对（仅首次需要，此后原地再生）。' 'Legacy Caddyfile without markers detected: marker pair inserted at the head of the site block (first run only; in-place regeneration afterwards).')
}

# ---------- 安全收尾：备份 → 写入 → validate → 失败回滚 → reload ----------
$caddyBak = "$caddyfile.bak"
Copy-Item -Path $caddyfile -Destination $caddyBak -Force
$authExisted = Test-Path $authFile
if ($authExisted) { Copy-Item -Path $authFile -Destination "$authFile.bak" -Force }

$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
# JSON：UTF-8 无 BOM（Rust 侧 serde_json 不剥 BOM）；Caddyfile 同编码（内容为 ASCII，与既有文件字节一致）
$jsonText = [pscustomobject]@{ accounts = $accounts } | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($authFile, $jsonText, $utf8NoBom)
[System.IO.File]::WriteAllLines($caddyfile, $lines, $utf8NoBom)

# caddy fmt --overwrite 规范化：消除每次 validate/adapt 的
# "Caddyfile input is not formatted" 警告（存量手改/空格缩进的 Caddyfile 一并治愈）。
# .bak 已备，回滚不受影响；fmt 失败不阻断——交给 validate 判死（格式问题属提示级）
$ErrorActionPreference = 'Continue'
& $caddy fmt --overwrite $caddyfile | Out-Null
$fmtCode = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($fmtCode -ne 0) {
    Write-Info (T "caddy fmt 规范化未执行（退出码 $fmtCode），不影响后续自检与生效。" "caddy fmt normalization skipped (exit $fmtCode); validation and activation unaffected.")
}

function Restore-Backups {
    Copy-Item -Path $caddyBak -Destination $caddyfile -Force
    if ($authExisted) {
        Copy-Item -Path "$authFile.bak" -Destination $authFile -Force
    } elseif (Test-Path $authFile) {
        Remove-Item -Path $authFile -Force   # 首次新增即失败：回到「无账号文件」原状
    }
}

# validate 前注入栈 .env 的腾讯云凭证（与 run-caddy-hidden.ps1 同款白名单），
# 使自检与真实运行同形（tls dns tencentcloud 引用 {env.*}）；
# 只进本进程环境，不落任何输出/日志
$envFile = Join-Path $StackDir '.env'
if (Test-Path $envFile) {
    foreach ($line in [System.IO.File]::ReadAllLines($envFile)) {
        if ($line -match '^\s*(TENCENT_SECRET_ID|TENCENT_SECRET_KEY)\s*=(.*)$') {
            Set-Item -Path ('env:' + $Matches[1]) -Value $Matches[2].Trim()
        }
    }
}

# PS 5.1 坑：EAP=Stop 下重定向原生命令 stderr 会把首行当异常抛出——
# validate/reload 段临时放宽（退出码才是判据，stderr 直落控制台供排障）
Write-Info (T '自检：caddy validate（失败将自动回滚本次全部写入）...' 'Self-check: caddy validate (all writes roll back automatically on failure)...')
$ErrorActionPreference = 'Continue'
& $caddy validate --config $caddyfile
$validateCode = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($validateCode -ne 0) {
    Restore-Backups
    Write-Bad (T "caddy validate 未通过（退出码 $validateCode）：已回滚 Caddyfile 与 auth-accounts.json（.bak 恢复），本次变更未生效。" "caddy validate failed (exit $validateCode): Caddyfile and auth-accounts.json rolled back from .bak; this change was NOT applied.")
    exit 1
}
Write-Ok (T 'caddy validate 通过。' 'caddy validate passed.')

# reload：admin API 本地回环（无需提权）。**先守卫再动手**——caddy reload 打的是
# 默认管理口 localhost:2019，与本脚本管理哪个栈无关；本机若跑着别的栈的 caddy
# （或同机演练多栈），盲 reload 会把别栈进程热加载成本栈配置。守卫条件：确有
# 正在运行的 caddy.exe 进程以本栈 Caddyfile 为 --config 实参（Win32_Process 只读
# 查询；工作台托管/自启链/总控菜单三条启动路径的命令行均含该路径）。
# 无匹配进程（caddy 未运行/属别栈）→ 「已保存，下次启动 caddy 时生效」（非错误）。
$caddyRunning = @(
    Get-CimInstance Win32_Process -Filter "Name='caddy.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -and ($_.CommandLine -like "*$caddyfile*") }
)
if ($caddyRunning.Count -gt 0) {
    $ErrorActionPreference = 'Continue'
    & $caddy reload --config $caddyfile
    $reloadCode = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($reloadCode -eq 0) {
        Write-Ok (T '已零停机生效（caddy reload）。' 'Applied with zero downtime (caddy reload).')
    } else {
        Write-Ok (T '已保存，下次启动 caddy 时生效（reload 未成功——caddy 管理口不可达，非错误）。' 'Saved; it takes effect the next time caddy starts (reload failed - the caddy admin API is unreachable - not an error).')
    }
} else {
    Write-Ok (T '已保存，下次启动 caddy 时生效（当前无以本栈配置运行的 caddy）。' 'Saved; it takes effect the next time caddy starts (no caddy is running with this stack''s config).')
}

# 汇总（绝不输出哈希/密码，仅用户名与计数）
$actionText = @{
    add    = (T '新增账号' 'added account')
    set    = (T '修改密码' 'changed password for')
    remove = (T '移除账号' 'removed account')
}[$Action]
Write-Ok (T "已${actionText}「$Username」：auth-accounts.json 现有 $($accounts.Count) 个账号。" "Done - ${actionText} '$Username': auth-accounts.json now holds $($accounts.Count) account(s).")
if ($accounts.Count -gt 0) {
    Write-Info (T ('账号清单：' + (@($accounts | ForEach-Object { $_.username }) -join ', ')) ('Accounts: ' + (@($accounts | ForEach-Object { $_.username }) -join ', ')))
    Write-Info (T '浏览器访问 https://<域名> 将弹出登录框；不同成员设备建议持不同账号（可单独移除吊销）。' 'Visiting https://<domain> now prompts for login; member devices should each use their own account (removable individually).')
} else {
    Write-Info (T '账号列表已清空：443 回到无密码门状态（维持现状语义）。' 'Account list is now empty: port 443 is back to no-password-gate behavior (status quo).')
}
if ($Action -eq 'remove') {
    Write-Info (T '移除只影响新请求：既有长连接断开后无法重连；必要时重启 caddy 彻底断开。' 'Removal affects new requests only: existing long-lived connections cannot reconnect after they drop; restart caddy to cut them immediately.')
}
