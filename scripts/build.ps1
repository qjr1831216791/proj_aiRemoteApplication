#Requires -Version 5.1
<#
.SYNOPSIS
  一键构建 AI 远程工作台双形态分发产物（specs/001-desktop-console T17）。

.DESCRIPTION
  流程（spec §5「打包分发」、plan §2/§8 裁决）：
    1. 版本对齐校验：package.json / Cargo.toml / tauri.conf.json 三处一致才继续
    2. 同步 sprint0 脚本子集 → src-tauri/resources/bin（清单 + SHA256 逐文件比对，打印差异）
    3. npm run tauri build（NSIS：embedBootstrapper + installMode currentUser）
    4. 便携 zip：exe + resources（目录结构与安装版一致）+ 说明 README（含 SmartScreen 引导）
    5. 产物复制到 release/ 并命名 <app>_<版本>_<arch>（app=AI-Remote-Workbench，arch=x64）
    6. 输出各产物体积清单

  产物二进制不入库（release/ 已被 .gitignore 忽略）。
  脚本契约：apps/workbench/src-tauri/src/scripts.rs 的 ScriptLocator 在安装版/
  便携版都按「exe 同级 resources\bin」定位内置副本，本脚本产出的目录结构须保持一致。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { <# 无控制台环境也不阻断 #> }

# 环境净化：用户装过 Store 版 PowerShell 7 时，其 WindowsApps 模块目录会混入
# PSModulePath，令 Windows PowerShell 5.1 的 cmdlet 自动加载失效（Get-FileHash/
# Compress-Archive 报 CommandNotFound）。剔除后恢复系统模块路径优先。
if ($env:PSModulePath -like '*windowsapps*') {
    $env:PSModulePath = (($env:PSModulePath -split ';') |
        Where-Object { $_ -notlike '*windowsapps*' }) -join ';'
}

# ── 布局常量 ────────────────────────────────────────────────────────────────
$RepoRoot  = Split-Path -Parent $PSScriptRoot          # scripts/ 的上级 = 仓库根
$AppDir    = Join-Path $RepoRoot   'apps/workbench'
$SrcBin    = Join-Path $RepoRoot   'tools/sprint0/bin' # 源：仓库脚本（只读，不改）
$DestBin   = Join-Path $AppDir     'src-tauri/resources/bin' # 内置副本（打包进产物）
$ReleaseDir = Join-Path $RepoRoot  'release'
$AppName   = 'AI-Remote-Workbench'
$Arch      = 'x64'

# 打包子集（plan §5.2 脚本契约 = Script 枚举全部 .ps1；stop-server.ps1 除外——
# 停止走程序内等效实现（spec §4.1），脚本保留为仓库内命令行兜底，不入产物）
$ScriptSubset = @(
    'run-server-hidden.ps1'   # 哨兵：ScriptLocator 以它判定目录有效
    'setup-autostart.ps1'     # 服务自启任务开/关（-Remove）
    'install-server.ps1'      # 安装/升级 CloudCLI（UAC）
    'install-https.ps1'       # HTTPS 栈装机（UAC）
    'enable-https.ps1'        # HTTPS 环境配置（UAC）
    'install-client.ps1'      # 客户端配置（交互式）
    'reset-ddns-password.ps1' # ddns-go 密码重置（交互式，spec 003）
    'set-frp-key.ps1'         # SakuraFrp 访问密钥写入 .env（交互式，spec 004）
    'set-tencent-key.ps1'     # 腾讯云 CAM 密钥写入 .env（交互式，spec 006）
    'config-ddnsgo.ps1'       # ddns-go 配置生成 + 拉起（spec 006）
)

function Write-Step { param([string]$Msg) Write-Host "`n==> $Msg" -ForegroundColor Cyan }
function Write-Ok   { param([string]$Msg) Write-Host "    $Msg" -ForegroundColor Green }
function Write-Diff { param([string]$Msg) Write-Host "    $Msg" -ForegroundColor Yellow }

# ── 1. 版本对齐校验（三处一致才继续；spec §5 打包分发）──────────────────────
Write-Step '版本对齐校验（package.json / Cargo.toml / tauri.conf.json）'

$pkgJson = Get-Content (Join-Path $AppDir 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$cargoToml = Get-Content (Join-Path $AppDir 'src-tauri/Cargo.toml') -Raw -Encoding UTF8
$cargoVer = if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') { $Matches[1] } else { $null }
$confJson = Get-Content (Join-Path $AppDir 'src-tauri/tauri.conf.json') -Raw -Encoding UTF8 | ConvertFrom-Json

$versions = [ordered]@{
    'package.json'    = $pkgJson.version
    'Cargo.toml'      = $cargoVer
    'tauri.conf.json' = $confJson.version
}
$versions.GetEnumerator() | ForEach-Object { Write-Host ("    {0,-16} {1}" -f $_.Key, $_.Value) }

$distinct = @($versions.Values | Select-Object -Unique)
if ($distinct.Count -ne 1 -or -not $distinct[0]) {
    throw "版本不一致：$($versions.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" } -join ' ')。请先对齐三处版本再构建。"
}
$Version = $distinct[0]
Write-Ok "三处一致：v$Version"

# ── 2. 同步脚本副本（清单 + 逐文件 SHA256 比对，打印差异；plan §7 脚本版本耦合对策）
Write-Step '同步 sprint0 脚本子集 -> src-tauri/resources/bin'

if (-not (Test-Path (Join-Path $SrcBin 'run-server-hidden.ps1'))) {
    throw "源脚本目录不可用：$SrcBin（缺哨兵 run-server-hidden.ps1）"
}
New-Item -ItemType Directory -Force -Path $DestBin | Out-Null

$changed = 0
foreach ($name in $ScriptSubset) {
    $src = Join-Path $SrcBin $name
    $dst = Join-Path $DestBin $name
    if (-not (Test-Path $src)) { throw "源脚本缺失：$src" }
    $srcHash = (Get-FileHash $src -Algorithm SHA256).Hash
    if (-not (Test-Path $dst)) {
        Copy-Item $src $dst -Force
        $changed++
        Write-Diff "[新增]     $name"
    }
    elseif ((Get-FileHash $dst -Algorithm SHA256).Hash -ne $srcHash) {
        Copy-Item $src $dst -Force
        $changed++
        Write-Diff "[已更新]   $name"
    }
    else {
        Write-Host "    [一致]     $name"
    }
    # 复制后复核（防拷贝中断产生半写文件）
    if ((Get-FileHash $dst -Algorithm SHA256).Hash -ne $srcHash) { throw "复制后哈希不符：$name" }
}

# 清理子集之外的陈旧 .ps1（manifest.json 不是脚本，不动）
Get-ChildItem $DestBin -Filter '*.ps1' -File -ErrorAction SilentlyContinue |
    Where-Object { $ScriptSubset -notcontains $_.Name } |
    ForEach-Object {
        Remove-Item $_.FullName -Force
        $changed++
        Write-Diff "[已移除]   $($_.Name)（不在打包子集）"
    }

# 内置副本清单（app 版本对齐 + 逐文件哈希；plan §7「内置副本打版本号对齐 app 版本」）
$manifest = [ordered]@{
    appVersion = $Version
    source     = 'tools/sprint0/bin'
    files      = [ordered]@{}
}
foreach ($name in $ScriptSubset) {
    $manifest.files[$name] = (Get-FileHash (Join-Path $SrcBin $name) -Algorithm SHA256).Hash.ToLowerInvariant()
}
$manifestJson = $manifest | ConvertTo-Json -Depth 4
$manifestPath = Join-Path $DestBin 'manifest.json'
$manifestChanged = -not (Test-Path $manifestPath) -or
    ((Get-Content $manifestPath -Raw -Encoding UTF8).Trim() -ne $manifestJson.Trim())
[System.IO.File]::WriteAllText($manifestPath, $manifestJson + "`r`n", (New-Object System.Text.UTF8Encoding($true)))
if ($manifestChanged) { Write-Diff "[清单]     manifest.json 已刷新（appVersion=$Version）" }
else { Write-Host '    [清单]     manifest.json 无变化' }
if ($changed -eq 0 -and -not $manifestChanged) { Write-Ok '脚本副本与源完全一致（无差异）' }
else { Write-Ok "同步完成：$changed 个文件变更" }

# ── 3. tauri build（NSIS；embedBootstrapper + currentUser 在 tauri.conf.json 确认）
Write-Step "npm run tauri build（NSIS，v$Version）—— 全量 release 构建，预计 5~15 分钟"

# cargo 不一定在系统 PATH（CLAUDE.md/开发机约定：~/.cargo/bin）
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

Push-Location $AppDir
try {
    & npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw "tauri build 失败（exit $LASTEXITCODE）" }
}
finally { Pop-Location }

$BundleDir = Join-Path $AppDir 'src-tauri/target/release/bundle'
$SetupExe  = Join-Path $BundleDir "nsis/$AppName`_$Version`_$Arch-setup.exe"
$BuiltExe  = Join-Path $AppDir 'src-tauri/target/release/ai-remote-workbench.exe'
foreach ($f in @($SetupExe, $BuiltExe)) {
    if (-not (Test-Path $f)) { throw "构建产物缺失：$f（检查上方构建日志）" }
}
Write-Ok "NSIS 安装器：$SetupExe"

# ── 4. 便携 zip（目录结构与安装版一致：exe + resources\bin + 说明 README）
Write-Step '组装便携包（exe + resources，目录结构与安装版一致）'

$StagingRoot   = Join-Path $ReleaseDir '.staging-portable'
$PortableInner = Join-Path $StagingRoot "$AppName`_$Version`_$Arch"
if (Test-Path $StagingRoot) { Remove-Item $StagingRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path "$PortableInner/resources" | Out-Null

Copy-Item $BuiltExe (Join-Path $PortableInner 'ai-remote-workbench.exe') -Force
Copy-Item $DestBin "$PortableInner/resources/bin" -Recurse -Force

# 便携包说明 README（spec §5 分发项：SmartScreen「仍要运行」引导必须可读）
$readmeText = @"
AI 远程工作台（$AppName）v$Version 便携版
================================================

这是什么
--------
CloudCLI / Caddy / ddns-go 三组件的 Windows 桌面控制台：状态一览、一键启停、
自启托管、装机与 HTTPS 配置入口。便携版解压即用，无需安装。

快速开始
--------
1. 把整个文件夹解压到任意“可写”目录（exe 与 resources\bin 必须保持同级，缺一不可）。
2. 双击 ai-remote-workbench.exe —— 程序常驻系统托盘，左键托盘图标可「显示主界面」。
3. 退出：托盘右键菜单 →「退出」（默认保留三组件运行）或「停止服务并退出」。

Windows SmartScreen 提示（未签名分发的正常现象）
------------------------------------------------
双击 exe 后若弹出“Windows 已保护你的电脑”：
    点「更多信息」→ 点「仍要运行」即可。
本程序当前未做代码签名（自用定位，spec §5 分发约束），杀毒软件误报时可对本目录添加信任。

目录说明
--------
    ai-remote-workbench.exe     主程序
    resources\bin\*.ps1         内置的 sprint0 脚本副本（启停/自启/装机/HTTPS 配置）。
                                请勿删除——脚本缺失时启动/停止/自启等功能会被禁用。
    resources\bin\manifest.json 脚本副本清单（版本与哈希，用于核对与仓库源的一致性）
    设置文件                     %APPDATA%\ai-remote-workbench\settings.json（与安装版共用）

---------------------------------------- 以下为英文说明 (English) --------
$AppName (portable) v$Version
Desktop control plane for CloudCLI / Caddy / ddns-go on Windows.
Unzip the WHOLE folder to a writable location (keep the exe next to
resources\bin), then run ai-remote-workbench.exe. The app lives in the
system tray; right-click the tray icon to show the UI or exit.
SmartScreen may warn "Windows protected your PC" because the build is
unsigned: click "More info" -> "Run anyway" (see spec for details).
Settings live in %APPDATA%\ai-remote-workbench\settings.json.
"@
[System.IO.File]::WriteAllText((Join-Path $PortableInner 'README.txt'), $readmeText, (New-Object System.Text.UTF8Encoding($true)))

# ── 5. 产物复制到 release/ 并统一命名 <app>_<版本>_<arch> ───────────────────
Write-Step '产物落位 release/ 并输出体积清单'

New-Item -ItemType Directory -Force -Path $ReleaseDir | Out-Null
$ReleaseSetup = Join-Path $ReleaseDir "$AppName`_$Version`_$Arch-setup.exe"
$ReleaseZip   = Join-Path $ReleaseDir "$AppName`_$Version`_$Arch.zip"
Copy-Item $SetupExe $ReleaseSetup -Force
Compress-Archive -Path $PortableInner -DestinationPath $ReleaseZip -Force
Remove-Item $StagingRoot -Recurse -Force   # 清理 staging

# ── 6. 体积清单（tasks.md「备注（T17 分发产物）」的数据源）───────────────────
$artifacts = @(
    @{ Name = "$AppName`_$Version`_$Arch-setup.exe"; Path = $ReleaseSetup; Kind = 'NSIS 安装器（embedBootstrapper / currentUser）' }
    @{ Name = "$AppName`_$Version`_$Arch.zip";       Path = $ReleaseZip;   Kind = '便携包（exe + resources + README）' }
)
$summary = @()
foreach ($a in $artifacts) {
    $mb = [math]::Round((Get-Item $a.Path).Length / 1MB, 2)
    $summary += ("    {0}  {1,8:N2} MB   {2}" -f $a.Name, $mb, $a.Kind)
}
Write-Host ''
Write-Host '    ── 分发产物（release/）────────────────────────────────────────'
$summary | ForEach-Object { Write-Host $_ -ForegroundColor Green }
Write-Host ''
Write-Ok "构建完成：$AppName v$Version（$Arch）双形态产物已就绪 -> $ReleaseDir"
