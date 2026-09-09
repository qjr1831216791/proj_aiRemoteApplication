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

/** 访问通道（spec 004）：直连为默认，穿透为 opt-in */
export type AccessChannel = "direct" | "tunnel";

/** 穿透配置（非敏感部分；访问密钥只存栈目录 .env，永不进入本结构） */
export interface TunnelConfig {
  tunnelId: string;
  nodeDomain: string;
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

/** DNS 对齐结论（check_dns_alignment 载荷；tag="kind" camelCase） */
export type DnsAlignment =
  | { kind: "alignedTunnel" }
  | { kind: "alignedDirect" }
  | { kind: "mismatchedCname"; actual: string }
  | { kind: "noRecord" }
  | { kind: "queryFailed" };

/** 三端访问地址（get_urls 载荷） */
export interface AccessUrls {
  local: string;
  lan: string;
  domain: string;
}

/** open_external 目标类别 */
export type ExternalKind = "workbench" | "local" | "lan" | "domain" | "ddns_admin";

/** run_tool 工具类别 */
export type ToolKind =
  | "install_server"
  | "install_https"
  | "enable_https"
  | "install_client"
  | "reset_ddns_password"
  | "set_frp_key";

/** run_tool 可选项（仅 install_server 消费） */
export interface ToolOpts {
  update: boolean;
  mirror: boolean;
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
