/**
 * 中文词条（与 en.ts 键集严格一致；新增键必须两边同步）。
 * Rust 侧原生文案（托盘菜单/状态卡 detail）在 src-tauri/src/lang.rs 同源维护。
 */
export const zh = {
  // 应用骨架
  "app.title": "AI 远程工作台",
  "app.subtitle": "手机遥控开发机上的 Claude Code",
  "nav.main": "主界面",
  "nav.settings": "设置",

  // 通用
  "common.start": "启动",
  "common.stop": "停止",
  "common.retry": "重试",
  "common.copy": "复制",
  "common.copied": "已复制",
  "common.open": "打开",
  "common.loading": "加载中…",
  "common.running": "运行中",
  "common.stopped": "未运行",
  "common.starting": "启动中",
  "common.failed": "失败",
  "common.portHeld": "端口被占",

  // 组件名
  "component.cloudcli": "CloudCLI",
  "component.caddy": "Caddy",
  "component.ddnsgo": "ddns-go",

  // 主界面：总开关与状态卡
  "main.startAll": "启动全部",
  "main.allRunning": "全部运行中",
  "main.allRunningHint": "三组件均在运行，无需启动",
  "main.stopAll": "停止全部",
  "main.stopHint": "停止会结束进行中的 AI 会话",
  "main.busy": "操作进行中…",
  "main.elapsed": "已持续",
  "main.port": "端口",
  "main.startingWait": "最长等待 60s，超时判失败",
  "main.goInstall": "去安装",

  // 地址区
  "addr.title": "访问地址",
  "addr.local": "本机",
  "addr.lan": "局域网",
  "addr.domain": "域名",

  // 低频操作区
  "tools.title": "低频操作",
  "tools.hint": "装机与排障入口，平时收起即可",
  "tools.installServer": "安装/重装服务端",
  "tools.installServerDesc": "install-server.ps1（管理员）",
  "tools.updateCloudcli": "升级 CloudCLI",
  "tools.updateCloudcliDesc": "install-server.ps1 -Update（管理员）",
  "tools.useMirror": "使用国内镜像源",
  "tools.installHttps": "HTTPS 栈装机",
  "tools.installHttpsDesc": "install-https.ps1（管理员，结尾有两步手工活）",
  "tools.enableHttps": "HTTPS 环境配置",
  "tools.enableHttpsDesc": "enable-https.ps1（管理员：防火墙/hosts）",
  "tools.installClient": "客户端配置",
  "tools.installClientDesc": "install-client.ps1（其他电脑/手机，交互式）",
  "tools.resetDdnsPassword": "重置 ddns-go 密码",
  "tools.resetDdnsPasswordDesc": "忘记密码时用（弹窗输入新密码，无需管理员）",
  "tools.openDdnsAdmin": "ddns-go 管理页",
  "tools.openDdnsAdminDesc": "DNS 解析与密钥管理",
  "tools.openWorkbench": "打开工作台页面",
  "tools.openWorkbenchDesc": "浏览器打开 https://ai.jackqi.cn",
  "tools.scriptsUnavailable": "脚本目录不可用，相关操作已禁用：",
  "tools.uacHint": "带「管理员」标记的操作会弹出 UAC 授权窗口",
  "tools.dispatched": "已派发：请在弹出的窗口中按提示完成操作",

  // 设置页
  "settings.behavior": "行为设置",
  "settings.autostartServices": "服务开机自启",
  "settings.autostartServicesDesc": "登录时由计划任务拉起三组件",
  "settings.autostartApp": "程序开机自启",
  "settings.autostartAppDesc": "登录时静默启动本程序并常驻托盘",
  "settings.linkStart": "启动时联动补齐服务",
  "settings.linkStartDesc": "程序启动时自动补齐未运行的组件（含手动启动）",
  "settings.exitAction": "退出行为",
  "settings.exitKeep": "保留服务",
  "settings.exitStop": "停止服务",
  "settings.openPageOnStart": "启动后打开工作台页面",
  "settings.openPageOnStartDesc": "程序启动完成后自动用浏览器打开工作台",
  "settings.language": "界面语言",
  "settings.langAuto": "跟随系统",
  "settings.langZh": "中文",
  "settings.langEn": "English",
  "settings.readonly": "端口与路径（只读）",
  "settings.readonlyHint": "修改端口/路径须重跑安装脚本，程序内不提供修改",
  "settings.portCloudcli": "CloudCLI 端口",
  "settings.portCaddy": "Caddy 端口",
  "settings.portDdnsgo": "ddns-go 端口",
  "settings.stackDir": "安装目录",
  "settings.domain": "域名",
  "settings.openLogs": "打开日志目录",
  "settings.tookOver": "已接管已存在的开机自启任务",
  "settings.repaired": "设置文件损坏，已恢复默认值",

  // 提示（toast）
  "toast.opFailed": "操作失败",
  "toast.saveFailed": "保存失败",
  "toast.loadFailed": "加载失败",
  "toast.copyFailed": "复制失败，请手动选择复制",
} as const;

export type DictKey = keyof typeof zh;
