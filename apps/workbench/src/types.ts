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
}

/** 补丁（save_settings 入参；只提交要改的字段） */
export interface SettingsPatch {
  language?: LanguageSetting;
  autostartServices?: boolean;
  autostartApp?: boolean;
  linkStartServices?: boolean;
  exitAction?: ExitAction;
  openPageOnStart?: boolean;
}

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
  | "install_client";

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

/** set_autostart_services 返回载荷 */
export interface TookOverPayload {
  tookOver: boolean;
}
