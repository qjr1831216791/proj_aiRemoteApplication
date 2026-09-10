/** Tauri 命令与事件的类型化封装（plan §5.1；命令层薄，逻辑在 Rust 侧） */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AccessChannel,
  AccessUrls,
  ComponentId,
  ComponentStatus,
  DnsAlignment,
  DomainHealth,
  ExternalKind,
  LanguageSetting,
  NetStatus,
  NetCategory,
  ProbeOutcome,
  ScriptsAvailability,
  Settings,
  SettingsPatch,
  TookOverPayload,
  ToolKind,
  ToolOpts,
  TunnelStatus,
} from "./types";

export const api = {
  isHiddenStartup: () => invoke<boolean>("is_hidden_startup"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (patch: SettingsPatch) => invoke<Settings>("save_settings", { patch }),
  /** 语言切换：Rust 侧托盘重建 + 脚本 -Lang 对齐（AC25 立即生效） */
  setLanguage: (setting: LanguageSetting) => invoke<Settings>("set_language", { setting }),

  getStatus: () => invoke<ComponentStatus[]>("get_status"),
  getUrls: () => invoke<AccessUrls>("get_urls"),
  startAll: () => invoke<void>("start_all"),
  stopAll: () => invoke<void>("stop_all"),
  startOne: (id: ComponentId) => invoke<void>("start_one", { id }),
  stopOne: (id: ComponentId) => invoke<void>("stop_one", { id }),

  openExternal: (kind: ExternalKind) => invoke<void>("open_external", { kind }),
  runTool: (kind: ToolKind, opts: ToolOpts) => invoke<void>("run_tool", { kind, opts }),
  scriptsAvailability: () => invoke<ScriptsAvailability>("scripts_availability"),
  openLogsDir: () => invoke<void>("open_logs_dir"),

  setAutostartServices: (enable: boolean) =>
    invoke<TookOverPayload>("set_autostart_services", { enable }),
  setAutostartApp: (enable: boolean) => invoke<void>("set_autostart_app", { enable }),

  /** 网络环境快照（spec 002）：即时探测；null = 尚无成功探测 */
  getNetStatus: () => invoke<NetStatus | null>("get_net_status"),
  /** 网络归类调整（spec 002 AC5/AC6）：UAC 提权派发，生效以 net://changed 复测为准；按网络名定位、序号兜底 */
  setNetworkCategory: (
    name: string,
    ifIndex: number,
    category: Extract<NetCategory, "private" | "public">,
  ) => invoke<void>("set_network_category", { name, ifIndex, category }),

  /** 隧道状态快照（spec 004；此后以 tunnel://status 事件为准） */
  getTunnelStatus: () => invoke<TunnelStatus>("get_tunnel_status"),
  /** 通道切换（AC5/6）：前置校验失败 → Err（未配置/已处于目标通道） */
  switchChannel: (target: AccessChannel) =>
    invoke<Settings>("switch_channel", { target }),
  /** 穿透启用开关（AC11） */
  setTunnelEnabled: (enabled: boolean) =>
    invoke<Settings>("set_tunnel_enabled", { enabled }),
  /** DNS 对齐检测（AC12/13）：权威 CNAME/A 实况 */
  checkDnsAlignment: () => invoke<DnsAlignment>("check_dns_alignment"),
  /** 打开栈目录（穿透设置指引链接） */
  openStackDir: () => invoke<void>("open_stack_dir"),
  /** 即时域名探测（通道体检；独立于 60s 心跳） */
  checkDomainHealthNow: () => invoke<ProbeOutcome>("check_domain_health_now"),
  /** 手动重启隧道（停止 → flushdns → 重新登录） */
  restartTunnel: () => invoke<TunnelStatus>("restart_tunnel"),
  /** Defender 白名单命令文本（frpc 两个运行位置；spec 004 分发保障） */
  getDefenderExclusionCmd: () => invoke<string>("get_defender_exclusion_cmd"),
  /** 一键恢复 frpc（官方 CDN 下载 → SHA256 校验 → 落位栈目录） */
  downloadFrpc: () => invoke<string>("download_frpc"),
};

/** 网络环境事件（Rust 侧 15s 轮询驱动，变化才发；spec 002 AC3） */
export function onNetChanged(cb: (status: NetStatus) => void): Promise<() => void> {
  return listen<NetStatus>("net://changed", (e) => cb(e.payload));
}

/** 状态事件（Rust 侧 2s 轮询器驱动；前端不另做轮询） */
export function onStatusChanged(
  cb: (statuses: ComponentStatus[]) => void,
): Promise<() => void> {
  return listen<ComponentStatus[]>("status://changed", (e) => cb(e.payload));
}

/** 隧道状态事件（spec 004：守护线程 5s 收敛驱动，变化才发） */
export function onTunnelStatus(cb: (status: TunnelStatus) => void): Promise<() => void> {
  return listen<TunnelStatus>("tunnel://status", (e) => cb(e.payload));
}

/** 域名心跳事件（spec 005：60s 周期探测，载荷 healthy 已含 2 次防抖） */
export function onDomainHealth(cb: (health: DomainHealth) => void): Promise<() => void> {
  return listen<DomainHealth>("domain://health", (e) => cb(e.payload));
}

/** 设置损坏恢复事件（AC24：非阻塞提示） */
export function onSettingsRepaired(
  cb: (payload: { backupPath: string }) => void,
): Promise<() => void> {
  return listen<{ backupPath: string }>("settings://repaired", (e) => cb(e.payload));
}

/** 复制到剪贴板：navigator.clipboard 优先，execCommand 兜底（WebView2 兼容） */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // 落入兜底路径
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}
