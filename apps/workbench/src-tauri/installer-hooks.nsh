; 安装器钩子（spec 004：frpc 随包分发于安装目录 resources\bin）。
; 覆盖安装前必须停止运行中的 frpc——它可能自上一次安装的 resources\bin 启动，
; 文件被占用会导致安装器写入失败/卡死（2026-09-09 真机实测）。
; 仅杀 frpc：caddy/ddns-go 居栈目录（ADR-0002 组件独立存活），安装器不触碰。

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "停止运行中的 frpc（避免更新时文件占用）..."
  nsExec::Exec 'taskkill /IM frpc.exe /F /T'
  Pop $0
  Sleep 500
!macroend
