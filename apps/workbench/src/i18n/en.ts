/**
 * English strings (key set must stay in sync with zh.ts).
 * Rust-side native texts (tray menu / status-card details) live in
 * src-tauri/src/lang.rs — keep them aligned.
 */
export const en = {
  // App shell
  "app.title": "AI Remote Workbench",
  "app.subtitle": "Drive Claude Code on your dev PC from any device",
  "nav.main": "Main",
  "nav.settings": "Settings",

  // Common
  "common.start": "Start",
  "common.stop": "Stop",
  "common.retry": "Retry",
  "common.copy": "Copy",
  "common.copied": "Copied",
  "common.open": "Open",
  "common.loading": "Loading…",
  "common.running": "Running",
  "common.stopped": "Stopped",
  "common.starting": "Starting",
  "common.failed": "Failed",
  "common.portHeld": "Port in use",

  // Components
  "component.cloudcli": "CloudCLI",
  "component.caddy": "Caddy",
  "component.ddnsgo": "ddns-go",

  // Main: master switch and status cards
  "main.startAll": "Start All",
  "main.allRunning": "All Running",
  "main.allRunningHint": "All three components are running",
  "main.stopAll": "Stop All",
  "main.stopHint": "Stopping ends any AI sessions in progress",
  "main.busy": "Working…",
  "main.elapsed": "for",
  "main.port": "Port",
  "main.startingWait": "up to 60s, then marked failed",
  "main.goInstall": "Set Up Environment",

  // Address area
  "addr.title": "Access Addresses",
  "addr.local": "This PC",
  "addr.lan": "LAN",
  "addr.domain": "Domain",

  // Low-frequency tools
  "tools.title": "Advanced Operations",
  "tools.hint": "Install & maintenance entries — collapsed by default",
  "tools.installServer": "Install / Reinstall Server",
  "tools.installServerDesc": "install-server.ps1 (admin)",
  "tools.updateCloudcli": "Update CloudCLI",
  "tools.updateCloudcliDesc": "install-server.ps1 -Update (admin)",
  "tools.useMirror": "Use China npm mirror",
  "tools.installHttps": "HTTPS Stack Setup",
  "tools.installHttpsDesc": "install-https.ps1 (admin; two manual steps at the end)",
  "tools.enableHttps": "HTTPS Environment Config",
  "tools.enableHttpsDesc": "enable-https.ps1 (admin: firewall / hosts)",
  "tools.installClient": "Client Setup",
  "tools.installClientDesc": "install-client.ps1 (other PC/phone, interactive)",
  "tools.resetDdnsPassword": "Reset ddns-go Password",
  "tools.resetDdnsPasswordDesc": "Forgot your password? Reset it (type in the popup, no admin)",
  "tools.openDdnsAdmin": "ddns-go Admin Page",
  "tools.openDdnsAdminDesc": "DNS records and credentials",
  "tools.openWorkbench": "Open Workbench Page",
  "tools.openWorkbenchDesc": "Open https://ai.jackqi.cn in the browser",
  "tools.scriptsUnavailable": "Scripts directory unavailable, related actions disabled:",
  "tools.uacHint": "Actions marked (admin) show a UAC prompt",
  "tools.dispatched": "Dispatched: follow the prompts in the opened window",

  // Settings
  "settings.behavior": "Behavior",
  "settings.autostartServices": "Autostart services at logon",
  "settings.autostartServicesDesc": "Scheduled tasks launch the three components",
  "settings.autostartApp": "Autostart this app at logon",
  "settings.autostartAppDesc": "Start silently to tray at logon",
  "settings.linkStart": "Link-start missing services",
  "settings.linkStartDesc": "Start missing components when the app starts (manual start included)",
  "settings.exitAction": "Exit behavior",
  "settings.exitKeep": "Keep services",
  "settings.exitStop": "Stop services",
  "settings.openPageOnStart": "Open workbench page after start",
  "settings.openPageOnStartDesc": "Open the workbench in the browser once the app starts",
  "settings.language": "Language",
  "settings.langAuto": "Follow system",
  "settings.langZh": "中文",
  "settings.langEn": "English",
  "settings.readonly": "Ports & Paths (read-only)",
  "settings.readonlyHint": "Changing ports/paths requires re-running the install script",
  "settings.portCloudcli": "CloudCLI port",
  "settings.portCaddy": "Caddy port",
  "settings.portDdnsgo": "ddns-go port",
  "settings.stackDir": "Install directory",
  "settings.domain": "Domain",
  "settings.openLogs": "Open Logs Directory",
  "settings.tookOver": "Existing autostart tasks were taken over",
  "settings.repaired": "Settings file was corrupted; defaults restored",

  // Toasts
  "toast.opFailed": "Operation failed",
  "toast.saveFailed": "Save failed",
  "toast.loadFailed": "Load failed",
  "toast.copyFailed": "Copy failed; please copy manually",
} as const;
