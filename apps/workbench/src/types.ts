/** 与 Rust 侧 schema 对齐的前端类型（plan §4 / §5.1） */

/** 组件标识（serde 字符串；ddnsgo 已随直连通道退役——spec 008） */
export type ComponentId = "cloudcli" | "caddy";

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

/** 访问通道（spec 008 二元化）：组网单通道——EasyTier 私有组网。
 * 直连/穿透已退役；旧设置文件的 direct/tunnel 值由 Rust 加载迁移为 mesh */
type AccessChannel = "mesh";

/** 组网配置（非敏感部分，spec 007 AC11；密钥只存栈目录 network-secret 文件，
 * 永不进入本结构/设置文件/命令行/日志——AC8） */
interface MeshConfig {
  networkName: string;
  virtualIp: string;
  virtualCidr: string;
  peers: string[];
}

/** 局域网边界守卫标记（spec 010 plan §4.1；sinceMs 由后端在派发成功后写入，
 * 前端只提交开关意图；旧设置文件缺字段 → false/0） */
interface LanGuardSettings {
  exceptionEnabled: boolean;
  exceptionSinceMs: number;
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
  mesh: MeshConfig;
  domainHeartbeat: boolean;
  stackDir: string;
  lanGuard: LanGuardSettings;
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
  mesh?: MeshConfig;
  domainHeartbeat?: boolean;
  stackDir?: string;
  lanGuard?: LanGuardSettings;
}

/** DNS 对齐结论（check_dns_alignment 载荷；tag="kind" camelCase；spec 008
 * 组网单通道：A 记录 = 虚拟 IP 为唯一对齐态，残留 CNAME 判旁路暴露面） */
export type DnsAlignment =
  | { kind: "alignedMesh" }
  | { kind: "mismatchedCname"; actual: string }
  | { kind: "mismatchedA"; actual: string }
  | { kind: "noRecord" }
  | { kind: "queryFailed" };

// ── 组网通道（spec 007）─────────────────────────────────────────────────────

/** 组网服务态（sc query 状态码 + sc qc 启动类型合成；camelCase） */
type MeshServiceState =
  | "notFound"
  | "running"
  | "startPending"
  | "stopped"
  | "disabled";

/** 组网状态五态（mesh://status 载荷的 state 字段；含非现役通道态） */
export type MeshStateKind = "online" | "connecting" | "offline" | "notConfigured" | "inactive";

/** 组网成员摘要（RPC 输出天然无密钥——AC8/AC4） */
interface MeshPeer {
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
type HealthKind = "ok" | "dns" | "connect" | "tls" | "timeout" | "status";

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

/** open_external 目标类别（ddns_admin 已随直连通道退役——spec 008） */
export type ExternalKind =
  | "workbench"
  | "local"
  | "lan"
  | "domain"
  | "easytier_releases";

/** run_tool 工具类别（ddns/frp 四类已随直连/穿透通道退役——spec 008） */
export type ToolKind =
  | "install_server"
  | "install_https"
  | "enable_https"
  | "install_client"
  | "set_tencent_key"
  | "set_mesh_secret";

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

/** 向导全量状态（wizard://changed 事件与各 wizard_* 命令的载荷；
 * branch 字段已随通道单化退役——spec 008，旧状态文件同名键被 Rust serde 忽略） */
export interface WizardState {
  version: number;
  stages: StageStatus[];
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
interface NetworkEntry {
  name: string;
  ifIndex: number;
  category: NetCategory;
}

/** 网络环境快照（get_net_status / net://changed 载荷；null = 尚无成功探测）。
 * spec 010 T5：002 的 rulePresent/rulePrivateOnly/alert 三字段随 443 归类告警
 * 链退役（plan §3.6），快照仅存活动网络行（供归类卡与 public_blocks_exception 消费） */
export interface NetStatus {
  networks: NetworkEntry[];
}

// ── 局域网边界守卫（spec 010）───────────────────────────────────────────────

/** 443 白名单健康五态（languard://changed 与 lan_guard_status 载荷）：
 * ok=三元全匹配 / missing=规则缺(TUN 在) / staleCidr|staleIface=失配待修复 /
 * dormant=TUN 未解析(组网不在,休眠非异常) */
export type WhitelistState = "ok" | "missing" | "staleCidr" | "staleIface" | "dormant";

/** 例外开关四态（tag="state"；on 携剩余秒数；expired=满 12h 回落未完成仍放行；
 * pending=已请求但规则未生效） */
export type ExceptionState =
  | { state: "off" }
  | { state: "on"; remainingSecs: number }
  | { state: "expired" }
  | { state: "pending" };

/** 白名单健康快照（languard://changed 事件与 lan_guard_status 命令载荷） */
export interface LanHealth {
  whitelist: WhitelistState;
  /** 任一旧规则存在 → 迁移横幅（AC10） */
  legacyPresent: boolean;
  exception: ExceptionState;
  /** 例外生效 ∧ 当前有公用活动网络（Private 规则直访不生效的如实提示） */
  publicBlocksException: boolean;
  /** 程序级旁路残留（AC11）：任一服务 exe 存在全端口放行规则 → 「旁路风险」
   * 警示 chip，经「修复白名单」清理（ensure-whitelist 语义已含程序规则清理） */
  bypassRisk: boolean;
}

/** set_autostart_services 返回载荷 */
export interface TookOverPayload {
  tookOver: boolean;
}
