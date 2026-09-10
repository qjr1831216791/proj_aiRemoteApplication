<#
.SYNOPSIS
  Caddy 隐藏常驻拉起（计划任务用）：先注入栈 .env 的腾讯云凭证，再前台运行 caddy run。
  Bilingual prompts follow the Windows display language; force with -Lang zh|en.

.DESCRIPTION
  spec 006 / ADR-0003：插件式 Caddyfile 以 {env.TENCENT_SECRET_ID} / {env.TENCENT_SECRET_KEY}
  引用凭证（不落明文）。计划任务环境没有这些变量 → 本脚本读取栈目录 .env 注入
  进程环境后，前台运行 caddy.exe run。
  必须保持长驻 run 而非 start：start 会 fork 子进程后退出，计划任务结束时 Windows
  会把同作业的子进程一并杀死，443 从未真正起来（§9.5-⑩）——故由本 powershell
  进程常驻，caddy 为其前台子进程（与 run-server-hidden.ps1 同款模式）。
  工作台托管的 Caddy 走 caddy_run（Rust 侧注入），不经过本脚本。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File .\run-caddy-hidden.ps1 -StackDir D:\Software\cloudcli-https
#>
[CmdletBinding()]
param(
    [string]$StackDir = 'D:\Software\cloudcli-https'
)

$ErrorActionPreference = 'Stop'

# 注入 {env.*} 凭证（只读文件、仅进程环境，不落任何输出/日志）
$envFile = Join-Path $StackDir '.env'
if (Test-Path $envFile) {
    foreach ($line in [System.IO.File]::ReadAllLines($envFile)) {
        if ($line -match '^\s*(TENCENT_SECRET_ID|TENCENT_SECRET_KEY)\s*=(.*)$') {
            Set-Item -Path ('env:' + $Matches[1]) -Value $Matches[2].Trim()
        }
    }
}

# 前台长驻（run 而非 start，§9.5-⑩）
& (Join-Path $StackDir 'caddy.exe') run --config (Join-Path $StackDir 'Caddyfile')
