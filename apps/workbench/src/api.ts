/** Tauri 命令与事件的类型化封装（plan §5.1；命令层薄，逻辑在 Rust 侧） */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AccessUrls,
  ComponentId,
  ComponentStatus,
  DnsAlignment,
  DomainHealth,
  ExternalKind,
  LanguageSetting,
  LanHealth,
  MeshDiagItem,
  MeshStatus,
  NetStatus,
  NetCategory,
  ProbeOutcome,
  ScriptsAvailability,
  Settings,
  SettingsPatch,
  TookOverPayload,
  ToolKind,
  ToolOpts,
  WizardState,
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

  /** 组网状态快照（spec 007；此后以 mesh://status 事件为准） */
  getMeshStatus: () => invoke<MeshStatus>("mesh_status"),
  /** 组网配置生效（AC1/AC9）：渲染 → 校验 → 落位 → UAC install/restart */
  meshApplyConfig: () => invoke<void>("mesh_apply_config"),
  /** 安装/刷新组网服务（AC3 前置；幂等建档，强制 install 动作） */
  meshInstallService: () => invoke<void>("mesh_install_service"),
  /** 卸载组网服务（停用 mesh 清理路径） */
  meshUninstallService: () => invoke<void>("mesh_uninstall_service"),
  /** 同步 DNS 到组网通道（AC13）：CNAME 全删 + A upsert 虚拟 IP；返回记录操作数 */
  meshSyncDns: () => invoke<number>("mesh_sync_dns"),
  /** 组网诊断（007 T16/AC13）：六项只读探测，可能耗时数秒（逐对端 3s 超时） */
  meshDiagnostics: () => invoke<MeshDiagItem[]>("mesh_diagnostics"),
  /** 成员入网配置（009 US4）：官方 TOML 文本（密钥为占位符 + 指引注释，无真实密钥） */
  meshMemberConfig: () => invoke<string>("mesh_member_config"),
  /** DNS 对齐检测（AC8：A=虚拟 IP 对齐 + 残留 CNAME 判旁路暴露面） */
  checkDnsAlignment: () => invoke<DnsAlignment>("check_dns_alignment"),

  // ── 局域网边界守卫（spec 010）─────────────────────────────────────────
  /** 白名单健康（即时探测；null = 尚无成功探测，此后以 languard://changed 为准） */
  lanGuardStatus: () => invoke<LanHealth | null>("lan_guard_status"),
  /** 例外开关（AC6/AC7）：UAC 派发成功才持久化；拒绝 → Err 状态原样 */
  lanGuardSetException: (on: boolean) =>
    invoke<void>("lan_guard_set_exception", { on }),
  /** 存量迁移「一键收口」（AC10）：UAC 派发 migrate（幂等删旧规则 + 就位白名单） */
  lanGuardMigrate: () => invoke<void>("lan_guard_migrate"),
  /** 失配修复「修复白名单」（AC1/AC9）：UAC 派发 ensure-whitelist */
  lanGuardEnsureWhitelist: () => invoke<void>("lan_guard_ensure_whitelist"),
  /** 打开栈目录（设置页「栈目录」链接） */
  openStackDir: () => invoke<void>("open_stack_dir"),
  /** 即时域名探测（通道体检；独立于 60s 心跳） */
  checkDomainHealthNow: () => invoke<ProbeOutcome>("check_domain_health_now"),

  // ── 装机向导（spec 006）───────────────────────────────────────────────
  wizardGetState: () => invoke<WizardState>("wizard_get_state"),
  /** 全量重探测（外部办理/脚本跑完后的统一「校验」入口） */
  wizardDetect: () => invoke<WizardState>("wizard_detect"),
  wizardSetDomain: (domain: string) => invoke<WizardState>("wizard_set_domain", { domain }),
  wizardComplete: () => invoke<WizardState>("wizard_complete"),
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

/** 组网状态事件（spec 007：观察者 5s 探询，变化才发；载荷同 mesh_status） */
export function onMeshStatus(cb: (status: MeshStatus) => void): Promise<() => void> {
  return listen<MeshStatus>("mesh://status", (e) => cb(e.payload));
}

/** 白名单健康事件（spec 010：60s 轮询 + 动作后即时刷新，变化才发） */
export function onLanGuardChanged(cb: (health: LanHealth) => void): Promise<() => void> {
  return listen<LanHealth>("languard://changed", (e) => cb(e.payload));
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

/** 向导状态事件（spec 006：set_domain/detect/complete 后推全量） */
export function onWizardChanged(cb: (state: WizardState) => void): Promise<() => void> {
  return listen<WizardState>("wizard://changed", (e) => cb(e.payload));
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
