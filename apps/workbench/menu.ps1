#Requires -Version 5.1
<#
.SYNOPSIS
  AI 远程工作台 · 傻瓜式操作菜单（双击同目录 start-here.bat 拉起）。

.DESCRIPTION
  数字选项覆盖日常开发动作：开发运行 / Rust 测试 / 前端构建 / 一键打包 / 打开产物 / 清理缓存 / 查看文档。
  重活（打包）委托仓库根 scripts\build.ps1 子进程执行，本脚本只做交互壳，不重复逻辑。
  编码约定：本文件 UTF-8 BOM + CRLF（.gitattributes）；start-here.bat 纯 ASCII。

.PARAMETER Lang
  zh | en，缺省跟随系统显示语言（zh → 中文，否则英文）。

.PARAMETER Run
  不进菜单直接执行对应选项号（脚本化友好），如 -Run 4 打包。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File menu.ps1
  powershell -NoProfile -ExecutionPolicy Bypass -File menu.ps1 -Lang en -Run 2
#>
[CmdletBinding()]
param(
    [ValidateSet('zh', 'en')]
    [string]$Lang = '',
    [int]$Run = -1
)

$ErrorActionPreference = 'Stop'
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { <# 无控制台环境不阻断 #> }

# ── 语言：-Lang 覆盖 > 系统显示语言 ─────────────────────────────────────────
if (-not $Lang) {
    $Lang = if ([System.Globalization.CultureInfo]::InstalledUICulture.Name -match '^zh') { 'zh' } else { 'en' }
}

$L = @{
    zh = @{
        title     = 'AI 远程工作台 · 操作菜单'
        envHead   = '环境自检'
        docs      = '帮助文档（可直接复制路径打开）'
        menu      = @(
            '  1. 开发运行桌面程序    （npm run tauri dev，首次自动 npm install）'
            '  2. 运行 Rust 测试      （cargo test，全套用例）'
            '  3. 前端构建检查        （npm run build，含 tsc 类型检查）'
            '  4. 一键打包分发        （安装器 + 便携 zip → 仓库 release\，委托 scripts\build.ps1）'
            '  5. 打开产物目录        （release\）'
            '  6. 清理构建缓存        （cargo clean + dist，保留 node_modules）'
            '  7. 查看帮助文档路径'
            '  0. 退出'
        )
        choose    = '输入选项数字'
        bye       = '再见。'
        emptyExit = '连续无输入，自动退出（防管道输入卡死）。'
        invalid   = '无效选项：{0}'
        ret       = '按 Enter 返回菜单...'
        done      = '—— 执行完成（退出码 {0}）——'
        fail      = '—— 执行失败（退出码 {0}），请回看上方输出排查 ——'
        nodeMiss  = '未检测到 node/npm：选项 1/3/4 不可用，请先安装 Node.js'
        cargoMiss = '未检测到 cargo（rustup 未装或不在 PATH）：选项 2/4/6 的 Rust 部分不可用'
        npmInst   = '[1/2] 首次运行，安装前端依赖（npm install）...'
        devStart  = '[2/2] 启动桌面程序（关闭程序窗口或此处 Ctrl+C 结束）...'
        packMiss  = '未找到打包脚本：{0}'
        noRel     = '产物目录还不存在：{0}（先执行选项 4 打包）'
        relIs     = '产物目录：'
        cleaned   = '已清理 cargo target 缓存与前端 dist。'
    }
    en = @{
        title     = 'AI Remote Workbench · Menu'
        envHead   = 'Environment check'
        docs      = 'Docs (copy a path to open)'
        menu      = @(
            '  1. Run desktop app (dev)   (npm run tauri dev, auto npm install on first run)'
            '  2. Run Rust tests          (cargo test, full suite)'
            '  3. Frontend build check    (npm run build, incl. tsc)'
            '  4. Package for distribution(installer + portable zip -> repo release\, delegates to scripts\build.ps1)'
            '  5. Open artifacts folder   (release\)'
            '  6. Clean build caches      (cargo clean + dist, keeps node_modules)'
            '  7. Show doc paths'
            '  0. Exit'
        )
        choose    = 'Choose an option number'
        bye       = 'Bye.'
        emptyExit = 'No input detected repeatedly, exiting (guards piped stdin).'
        invalid   = 'Invalid option: {0}'
        ret       = 'Press Enter to return to menu...'
        done      = '—— Done (exit code {0}) ——'
        fail      = '—— FAILED (exit code {0}), see output above ——'
        nodeMiss  = 'node/npm not found: options 1/3/4 unavailable, install Node.js first'
        cargoMiss = 'cargo not found (rustup missing or not in PATH): Rust parts of 2/4/6 unavailable'
        npmInst   = '[1/2] First run, installing frontend deps (npm install)...'
        devStart  = '[2/2] Starting desktop app (close the app window or Ctrl+C here to stop)...'
        packMiss  = 'Build script not found: {0}'
        noRel     = 'Artifact folder does not exist yet: {0} (run option 4 first)'
        relIs     = 'Artifact folder:'
        cleaned   = 'Cleaned cargo target cache and frontend dist.'
    }
}
$t = $L[$Lang]

# ── 环境：工作目录、cargo PATH 兜底、自检 ───────────────────────────────────
Set-Location $PSScriptRoot
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if ((Test-Path $cargoBin) -and (($env:PATH -split ';') -notcontains $cargoBin)) {
    $env:PATH = "$cargoBin;$env:PATH"
}

$hasNpm   = [bool](Get-Command npm   -ErrorAction SilentlyContinue)
$hasCargo = [bool](Get-Command cargo -ErrorAction SilentlyContinue)

Write-Host ''
Write-Host $t.title -ForegroundColor Cyan
Write-Host ('-' * 60)
Write-Host $t.envHead
$nodeVer = if ($hasNpm) { try { (node --version 2>$null) } catch { '?' } } else { '-' }
$npmVer  = if ($hasNpm) { try { (npm --version 2>$null) } catch { '?' } } else { '-' }
$carVer  = if ($hasCargo) { try { (cargo --version 2>$null) } catch { '?' } } else { '-' }
Write-Host ("  node : {0}" -f $nodeVer)
Write-Host ("  npm  : {0}" -f $npmVer)
Write-Host ("  cargo: {0}" -f $carVer)
if (-not $hasNpm)   { Write-Host ("  ! " + $t.nodeMiss)  -ForegroundColor Yellow }
if (-not $hasCargo) { Write-Host ("  ! " + $t.cargoMiss) -ForegroundColor Yellow }
Write-Host ('-' * 60)

# ── 动作 ────────────────────────────────────────────────────────────────────
function Invoke-Option {
    param([string]$Choice)
    switch ($Choice) {
        '1' {
            if (-not $hasNpm) { Write-Host $t.nodeMiss -ForegroundColor Yellow; return }
            if (-not (Test-Path (Join-Path $PSScriptRoot 'node_modules'))) {
                Write-Host $t.npmInst
                npm install
                if ($LASTEXITCODE -ne 0) { Write-Host ($t.fail -f $LASTEXITCODE) -ForegroundColor Red; return }
            }
            Write-Host $t.devStart
            npm run tauri dev
            Write-Host (& { if ($LASTEXITCODE -eq 0) { $t.done -f 0 } else { $t.fail -f $LASTEXITCODE } }) -ForegroundColor $(if ($LASTEXITCODE -eq 0) { 'Green' } else { 'Red' })
        }
        '2' {
            if (-not $hasCargo) { Write-Host $t.cargoMiss -ForegroundColor Yellow; return }
            cargo test --manifest-path src-tauri/Cargo.toml
            Write-Host (& { if ($LASTEXITCODE -eq 0) { $t.done -f 0 } else { $t.fail -f $LASTEXITCODE } }) -ForegroundColor $(if ($LASTEXITCODE -eq 0) { 'Green' } else { 'Red' })
        }
        '3' {
            if (-not $hasNpm) { Write-Host $t.nodeMiss -ForegroundColor Yellow; return }
            npm run build
            Write-Host (& { if ($LASTEXITCODE -eq 0) { $t.done -f 0 } else { $t.fail -f $LASTEXITCODE } }) -ForegroundColor $(if ($LASTEXITCODE -eq 0) { 'Green' } else { 'Red' })
        }
        '4' {
            if (-not $hasNpm) { Write-Host $t.nodeMiss -ForegroundColor Yellow; return }
            if (-not $hasCargo) { Write-Host $t.cargoMiss -ForegroundColor Yellow; return }
            $build = Join-Path $PSScriptRoot '..\..\scripts\build.ps1'
            if (-not (Test-Path $build)) { Write-Host ($t.packMiss -f $build) -ForegroundColor Red; return }
            # 子进程执行：隔离 build.ps1 的 exit 与其 PSModulePath 净化，取回退出码
            powershell -NoProfile -ExecutionPolicy Bypass -File $build
            Write-Host (& { if ($LASTEXITCODE -eq 0) { $t.done -f 0 } else { $t.fail -f $LASTEXITCODE } }) -ForegroundColor $(if ($LASTEXITCODE -eq 0) { 'Green' } else { 'Red' })
        }
        '5' {
            $rel = Join-Path $PSScriptRoot '..\..\release'
            if (Test-Path $rel) {
                Write-Host ($t.relIs + ' ' + $rel)
                Start-Process explorer.exe -ArgumentList "`"$rel`""
            } else {
                Write-Host ($t.noRel -f $rel) -ForegroundColor Yellow
            }
        }
        '6' {
            if ($hasCargo) { cargo clean --manifest-path src-tauri/Cargo.toml }
            if (Test-Path .\dist) { Remove-Item .\dist -Recurse -Force -ErrorAction SilentlyContinue }
            Write-Host $t.cleaned
        }
        '7' {
            Write-Host $t.docs
            @(
                (Join-Path $PSScriptRoot '..\..\README.md'),
                (Join-Path $PSScriptRoot '..\..\CLAUDE.md'),
                (Join-Path $PSScriptRoot '..\..\specs\001-desktop-console\spec.md'),
                (Join-Path $PSScriptRoot '..\..\specs\001-desktop-console\acceptance-manual.md'),
                (Join-Path $PSScriptRoot '..\..\scripts\build.ps1')
            ) | ForEach-Object { Write-Host ("  " + $_) }
        }
        default { Write-Host ($t.invalid -f $Choice) -ForegroundColor Yellow }
    }
}

# ── 主循环 ──────────────────────────────────────────────────────────────────
if ($Run -ge 0) {
    Invoke-Option ([string]$Run)
    exit 0
}

$empty = 0
while ($true) {
    Write-Host ''
    $t.menu | ForEach-Object { Write-Host $_ }
    $choice = Read-Host $t.choose
    if ([string]::IsNullOrWhiteSpace($choice)) {
        $empty++
        if ($empty -ge 3) { Write-Host $t.emptyExit; break }
        continue
    }
    $empty = 0
    if ($choice -eq '0') { break }
    Invoke-Option $choice.Trim()
    Read-Host $t.ret | Out-Null
}
Write-Host $t.bye
