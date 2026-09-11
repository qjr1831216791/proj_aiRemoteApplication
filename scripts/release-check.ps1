<#
.SYNOPSIS
  发版前防呆校验：三处版本号一致性 + CHANGELOG 版本节存在性（只读，不写任何文件）。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  机器化 CHANGELOG.md 底部「发布步骤」第 1/2 步的防呆检查（spec 009 US2，AC5~AC7）：
  0.3.0 曾漏 bump 三处版本文件（v0.4.0 跳号修正），此后发版前以本脚本一道校验。
  校验对象与判据（全程只读，绝不写盘）：
    1. apps/workbench/package.json               的 version
    2. apps/workbench/src-tauri/Cargo.toml       的 [package] 段内 version
       （正则锚定 [package] 段，遇下一个段头即止，不误中 [dependencies] 等段内的 version）
    3. apps/workbench/src-tauri/tauri.conf.json  的 version
    4. 仓库根 CHANGELOG.md                       存在 ## [<基准版本>] 版本节
       （## [Unreleased] 不算——须先按发布步骤收口为版本节）
  基准版本：显式 -Version 优先；缺省取 package.json 的 version。
  退出码：0 = 全部通过；1 = 任一失败（失败项指明各文件版本值/缺失节名）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-check.ps1
  # 以 package.json 的 version 为基准校验（日常用法）

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-check.ps1 -Version 0.5.0
  # 发版中：以目标版本为基准，校验三处均已 bump、CHANGELOG 已收口该版本节
#>
[CmdletBinding()]
param(
    [ValidatePattern('^\d+\.\d+\.\d+(-[\w.]+)?$')]
    [string]$Version,
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
function Write-Ok   { param([string]$Message) Write-Host "[OK] $Message" -ForegroundColor Green }
function Write-Info { param([string]$Message) Write-Host "[i ] $Message" -ForegroundColor DarkGray }
function Write-Bad  { param([string]$Message) Write-Host "[X ] $Message" -ForegroundColor Red }

# ---------- 路径（仓库根 = 本脚本所在 scripts/ 的上一级，任意 cwd 均可运行） ----------
$RepoRoot    = Split-Path -Parent $PSScriptRoot
$PackageJson = Join-Path $RepoRoot 'apps/workbench/package.json'
$CargoToml   = Join-Path $RepoRoot 'apps/workbench/src-tauri/Cargo.toml'
$TauriConf   = Join-Path $RepoRoot 'apps/workbench/src-tauri/tauri.conf.json'
$Changelog   = Join-Path $RepoRoot 'CHANGELOG.md'

# ---------- 取值（只读；读不到返回 $null，由逐项检查如实报告，不中断其余项） ----------
function Get-JsonVersion {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return $null }
    try { (Get-Content -Raw -Path $Path | ConvertFrom-Json).version } catch { $null }
}

function Get-CargoPackageVersion {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return $null }
    $inPackage = $false
    foreach ($line in [System.IO.File]::ReadAllLines($Path)) {
        if ($line -match '^\s*\[package\]\s*(#.*)?$') { $inPackage = $true; continue }
        if ($inPackage -and $line -match '^\s*\[') { break }  # 进入下一段，[package] 段结束
        if ($inPackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') { return $Matches[1] }
    }
    $null
}

$vPkg   = Get-JsonVersion          -Path $PackageJson
$vCargo = Get-CargoPackageVersion  -Path $CargoToml
$vTauri = Get-JsonVersion          -Path $TauriConf

# ---------- 基准版本 ----------
$script:failures = @()
$baseline = $Version
if (-not $baseline) {
    $baseline = $vPkg
    if (-not $baseline) {
        Write-Bad (T "未取到基准版本：$PackageJson 缺失或无 version 字段；可显式传 -Version X.Y.Z" "Baseline version unavailable: $PackageJson missing or has no version field; pass -Version X.Y.Z explicitly")
        exit 1
    }
    Write-Info (T "基准版本：$baseline（取自 package.json）" "Baseline version: $baseline (from package.json)")
} else {
    Write-Info (T "基准版本：$baseline（-Version 显式传入）" "Baseline version: $baseline (explicit -Version)")
}

# ---------- 逐项检查（4 项：三处版本 vs 基准 + CHANGELOG 版本节） ----------
$miss = T '未取到（文件缺失或无 version 字段）' 'unavailable (file missing or no version field)'

function Test-VersionItem {
    param([string]$Label, [string]$Path, [string]$Actual, [string]$Baseline)
    if ($Actual -and $Actual -eq $Baseline) {
        Write-Ok "$Label = $Actual"
    } else {
        $shown = if ($Actual) { $Actual } else { $script:miss }
        Write-Bad (T "$Label = $shown，与基准 $Baseline 不一致（$Path）" "$Label = $shown, mismatch with baseline $Baseline ($Path)")
        $script:failures += $Label
    }
}

Test-VersionItem -Label 'package.json version'         -Path $PackageJson -Actual $vPkg   -Baseline $baseline
Test-VersionItem -Label 'Cargo.toml [package] version' -Path $CargoToml   -Actual $vCargo -Baseline $baseline
Test-VersionItem -Label 'tauri.conf.json version'      -Path $TauriConf   -Actual $vTauri -Baseline $baseline

if (Test-Path $Changelog) {
    $sectionPattern = '^##\s*\[' + [regex]::Escape($baseline) + '\]'
    if (Select-String -Path $Changelog -Pattern $sectionPattern -Quiet) {
        Write-Ok (T "CHANGELOG.md 存在版本节 [$baseline]" "CHANGELOG.md has section [$baseline]")
    } else {
        Write-Bad (T "CHANGELOG.md 缺少版本节 [$baseline]——仅有 ## [Unreleased] 不算，请先按 CHANGELOG 底部「发布步骤」收口" "CHANGELOG.md lacks section [$baseline] - ## [Unreleased] does not count; close it out per the release steps at the bottom of CHANGELOG.md first")
        $script:failures += 'CHANGELOG section'
    }
} else {
    Write-Bad (T "CHANGELOG.md 不存在：$Changelog" "CHANGELOG.md not found: $Changelog")
    $script:failures += 'CHANGELOG file'
}

# ---------- 汇总（0 = 全过；1 = 任一失败） ----------
$total  = 4
$passed = $total - $script:failures.Count
Write-Host ''
if ($script:failures.Count -eq 0) {
    Write-Ok (T "全部 $total 项检查通过（$passed/$total）。" "All $total checks passed ($passed/$total).")
    exit 0
} else {
    Write-Bad (T "$($script:failures.Count)/$total 项失败：$($script:failures -join '、')。发版前请 bump 三处版本文件并收口 CHANGELOG 版本节（本脚本只读，不代改）。" "$($script:failures.Count)/$total checks failed: $($script:failures -join ', '). Bump the three version files and close out the CHANGELOG section before releasing (this script is read-only).")
    exit 1
}
