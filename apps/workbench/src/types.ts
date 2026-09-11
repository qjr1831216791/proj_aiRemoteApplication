/** 与 Rust 侧 schema 对齐的前端类型（plan §4 / §5.1） */

/** 组件标识（serde 字符串） */
export type ComponentId = "cloudcli" | "caddy" | "ddnsgo";

/** 组件五态（kebab-case，plan §4） */
export type ComponentState = "stopped" | "starting" | "running" | "port-held" | "failed";

export interface ComponentStatus {
  id: ComponentId;
  state: ComponentState;
  port: number;
  detail?: string;
  /** 进入该状态的 epoch 毫秒时间戳 */
  since: number;
}

export type LanguageSetting = "auto" | "zh" | "en";
export type ExitAction = "keep" | "stop";

/** 访问通道（spec 004 + 007）：direct = DDNS 直连，tunnel = SakuraFrp 穿透，
 * mesh = EasyTier 私有组网（007 新增，新装机默认推荐） */
export type AccessChannel = "direct" | "tunnel" | "mesh";

/** 穿透配置（非敏感部分；访问密钥只存栈目录 .env，永不进入本结构） */
export interface TunnelConfig {
  tunnelId: string;
  nodeDomain: string;
}

/** 组网配置（非敏感部分，spec 007 AC11；密钥只存栈目录 network-secret 文件，
 * 永不进入本结构/设置文件/命令行/日志——AC8） */
export interface MeshConfig {
  networkName: string;
  virtualIp: string;
  virtualCidr: string;
  peers: string[];
}

/** 全量设置（get_settings 载荷，camelCase 对齐 Rust serde） */
export interface Settings {
  version: number;
  language: LanguageSetting;
  autostartServices: boolean;
  autostartApp: boolean;
  linkStartServices: boolean;
  exitAction: ExitAction;
  openPageOnStart: boolean;
  scriptsDirOverride: string | null;
  accessChannel: AccessChannel;
  tunnel: TunnelConfig | null;
  tunnelEnabled: boolean;
  mesh: MeshConfig;
  /** 穿透通道已停用（spec 007 AC5：停用后 frpc 不被拉起，重新启用须安全警示确认） */
  tunnelDisabled: boolean;
  /** 直连通道已停用（spec 007 AC6） */
  directDisabled: boolean;
  domainHeartbeat: boolean;
  stackDir: string;
}

/** 补丁（save_settings 入参；只提交要改的字段） */
export interface SettingsPatch {
  language?: LanguageSetting;
  autostartServices?: boolean;
  autostartApp?: boolean;
  linkStartServices?: boolean;
  exitAction?: ExitAction;
  openPageOnStart?: boolean;
  accessChannel?: AccessChannel;
  tunnel?: TunnelConfig;
  tunnelEnabled?: boolean;
  mesh?: MeshConfig;
  tunnelDisabled?: boolean;
  directDisabled?: boolean;
  domainHeartbeat?: boolean;
  stackDir?: string;
}

/** 隧道运行状态（tunnel://status 载荷；tag="state" camelCase） */
export type TunnelStateKind =
  | "notConfigured"
  | "disabled"
  | "inactive"
  | "starting"
  | "online"
  | "offline";

export interface TunnelStatus {
  state: TunnelStateKind;
  detail?: string;
  since: number;
}

/** DNS 对齐结论（check_dns_alignment 载荷；tag="kind" camelCase；mesh 态新增
 * alignedMesh/mismatchedA——spec 007 体检重定义：A 记录 = 虚拟 IP） */
export type DnsAlignment =
  | { kind: "alignedTunnel" }
  | { kind: "alignedDirect" }
  | { kind: "alignedMesh" }
  | { kind: "mismatchedCname"; actual: string }
  | { kind: "mismatchedA"; actual: string }
  | { kind: "noRecord" }
  | { kind: "queryFailed" };

// ── 组网通道（spec 007）─────────────────────────────────────────────────────

/** 组网服务态（sc query 状态码 + sc qc 启动类型合成；camelCase） */
export type MeshServiceState =
  | "notFound"
  | "running"
  | "startPending"
  | "stopped"
  | "disabled";

/** 组网状态五态（mesh://status 载荷的 state 字段；含非现役通道态） */
export type MeshStateKind = "online" | "connecting" | "offline" | "notConfigured" | "inactive";

/** 组网成员摘要（RPC 输出天然无密钥——AC8/AC4） */
export interface MeshPeer {
  hostname: string;
  ipv4?: string;
  /** 延迟毫秒（本机/未测为空） */
  latencyMs?: number;
  /** 丢包率 0~1（已归一） */
  lossRate?: number;
  /** 本机自身（peer list 首项恒为本机；在线判定排除本机项） */
  isLocal: boolean;
}

/** 组网状态快照（mesh://status 事件与 mesh_status 命令载荷；detail 为稳定码） */
export interface MeshStatus {
  state: MeshStateKind;
  detail?: string;
  since: number;
  service: MeshServiceState;
  peers: MeshPeer[];
}

/** 组网诊断单项（007 T16/AC13：code → `mesh.diag.<code>.ok|bad` 双语文案；
 * detail 为数据摘要（host:port=状态 / 计数 / 解析值），不含密钥） */
export interface MeshDiagItem {
  code: "service" | "secret" | "peer_reachable" | "members" | "local_nic" | "domain_chain";
  ok: boolean;
  detail?: string;
}

/** 域名心跳快照（domain://health 载荷；healthy 已含 2 次防抖，spec 005 AC6） */
export type HealthKind = "ok" | "dns" | "connect" | "tls" | "timeout" | "status";

export interface DomainHealth {
  healthy: boolean;
  kind: HealthKind;
  code?: number;
  latencyMs: number;
  failures: number;
  since: number;
}

/** 单次即时探测（通道体检用） */
export interface ProbeOutcome {
  kind: HealthKind;
  code?: number;
  latencyMs: number;
}

/** 三端访问地址（get_urls 载荷） */
export interface AccessUrls {
  local: string;
  lan: string;
  domain: string;
}

/** open_external 目标类别 */
export type ExternalKind =
  | "workbench"
  | "local"
  | "lan"
  | "domain"
  | "ddns_admin"
  | "easytier_releases";

/** run_tool 工具类别 */
export type ToolKind =
  | "install_server"
  | "install_https"
  | "enable_https"
  | "install_client"
  | "reset_ddns_password"
  | "set_frp_key"
  | "set_tencent_key"
  | "config_ddnsgo"
  | "set_mesh_secret"
  | "clear_frp_key";

/** run_tool 可选项（update/mirror 仅 install_server；domain 供安装/配置类透传，spec 006） */
export interface ToolOpts {
  update: boolean;
  mirror: boolean;
  domain?: string | null;
}

// ── 装机向导（spec 006）─────────────────────────────────────────────────────

/** 向导阶段（顺序即推进顺序） */
export type WizardStageId = "basis" | "tencent" | "https" | "channel" | "finalize";

/** 阶段态（无 running：派发繁忙为前端局部状态） */
export type StageState = "pending" | "done" | "failed" | "skipped";

/** 单阶段状态（detail = 稳定码，双语归 i18n） */
export interface StageStatus {
  id: WizardStageId;
  state: StageState;
  detail?: string | null;
}

/** 向导全量状态（wizard://changed 事件与各 wizard_* 命令的载荷） */
export interface WizardState {
  version: number;
  stages: StageStatus[];
  branch: AccessChannel | null;
  domain: string;
  done: boolean;
}

/** 脚本可用性（spec §4.5：禁用原因透传） */
export interface ScriptsAvailability {
  available: boolean;
  reason: string | null;
}

/** 网络归类（spec 002：Windows NetworkCategory 映射） */
export type NetCategory = "public" | "private" | "domain" | "unknown";

/** 一条活动网络 */
export interface NetworkEntry {
  name: string;
  ifIndex: number;
  category: NetCategory;
}

/** 网络环境快照（get_net_status / net://changed 载荷；null = 尚无成功探测） */
export interface NetStatus {
  rulePresent: boolean;
  rulePrivateOnly: boolean;
  networks: NetworkEntry[];
  alert: boolean;
}

/** set_autostart_services 返回载荷 */
export interface TookOverPayload {
  tookOver: boolean;
}
