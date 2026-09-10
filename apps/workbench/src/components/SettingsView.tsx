/**
 * 设置页（T14：AC21/22/24/25 展示层）。
 * - 五项行为开关：两项自启开关先跑计划任务命令（tookOver → 提示）再持久化；
 *   联动补齐 / 退出行为 / 启动后打开页面为纯设置项，改即存
 * - 语言：跟随系统/中文/英文三选，切换立即生效（App 负责 Rust 托盘重建 + 全局换词典）
 * - 端口/路径/域名只读卡：一键复制 + "修改须重跑安装脚本"指引（AC22）
 * - 打开日志目录按钮；settings://repaired 事件提示在 App 层统一 toast
 * 全部乐观更新 + 失败回滚，长任务期间对应开关禁用防重复提交。
 */

import { useState } from "preact/hooks";
import { api, copyText } from "../api";
import { t, type Lang } from "../i18n";
import type { ExitAction, LanguageSetting, Settings, SettingsPatch } from "../types";
import { CopyButton } from "./CopyButton";

export interface SettingsViewProps {
  lang: Lang;
  settings: Settings;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
  /** 设置对象变更（乐观更新/回滚/服务端合并结果） */
  onSettingsChange: (s: Settings) => void;
  /** 语言切换（App：set_language → 托盘重建 + 前端全局换语言） */
  onLanguageChange: (setting: LanguageSetting) => Promise<void>;
}

// 只读展示常量：与 src-tauri/src/consts.rs 同步维护（改这里必须改那里，重跑安装脚本才生效）
const READONLY = {
  cloudcliPort: 3001,
  caddyPort: 443,
  ddnsgoPort: 9876,
  stackDir: "D:\\Software\\cloudcli-https",
  domain: "ai.jackqi.cn",
} as const;

export function SettingsView(props: SettingsViewProps) {
  const { lang, settings, onToast, onSettingsChange, onLanguageChange } = props;
  // 任务类开关在途标记（计划任务脚本最长 ~60s，期间禁用对应开关）
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [langBusy, setLangBusy] = useState(false);
  // 穿透设置表单（spec 004 AC14；初始值取已存配置，空串 = 未配置）
  const [tunnelId, setTunnelId] = useState(settings.tunnel?.tunnelId ?? "");
  const [nodeDomain, setNodeDomain] = useState(settings.tunnel?.nodeDomain ?? "");

  /** 任务类自启开关：先执行计划任务命令，成功后持久化到设置文件 */
  const runAutostartToggle = async (key: "autostartServices" | "autostartApp", enable: boolean) => {
    setBusyKey(key);
    const prev = settings;
    onSettingsChange(
      key === "autostartServices"
        ? { ...settings, autostartServices: enable }
        : { ...settings, autostartApp: enable },
    );
    try {
      if (key === "autostartServices") {
        const r = await api.setAutostartServices(enable);
        if (r.tookOver) onToast(t("settings.tookOver", lang), "info");
      } else {
        await api.setAutostartApp(enable);
      }
      const patch: SettingsPatch =
        key === "autostartServices" ? { autostartServices: enable } : { autostartApp: enable };
      onSettingsChange(await api.saveSettings(patch));
    } catch (e) {
      onSettingsChange(prev);
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setBusyKey(null);
    }
  };

  /** 纯设置项：改即存（保存失败回滚） */
  const savePatch = async (patch: SettingsPatch) => {
    const prev = settings;
    onSettingsChange({ ...settings, ...patch });
    try {
      onSettingsChange(await api.saveSettings(patch));
    } catch (e) {
      onSettingsChange(prev);
      onToast(`${t("toast.saveFailed", lang)}: ${String(e)}`, "error");
    }
  };

  const switchLanguage = async (setting: LanguageSetting) => {
    if (setting === settings.language || langBusy) return;
    setLangBusy(true);
    try {
      await onLanguageChange(setting);
    } finally {
      setLangBusy(false);
    }
  };

  /** 穿透配置保存（AC14/15）：校验 → 持久化；就绪判定实时生效（守护/切换入口读设置） */
  const saveTunnel = async () => {
    const id = tunnelId.trim();
    const dom = nodeDomain.trim();
    if (!/^\d+$/.test(id)) {
      onToast(t("settings.tunnelIdInvalid", lang), "error");
      return;
    }
    if (!dom) {
      onToast(t("settings.tunnelNodeRequired", lang), "error");
      return;
    }
    try {
      onSettingsChange(await api.saveSettings({ tunnel: { tunnelId: id, nodeDomain: dom } }));
      onToast(t("settings.tunnelSaved", lang), "success");
    } catch (e) {
      onToast(`${t("toast.saveFailed", lang)}: ${String(e)}`, "error");
    }
  };

  /** 访问密钥脚本（AC16）：拉起控制台交互窗，密钥经脚本直写 .env 不进 IPC/日志 */
  const openSetFrpKey = () => {
    api
      .runTool("set_frp_key", { update: false, mirror: false })
      .then(() => onToast(t("settings.setFrpKeyDispatched", lang), "info"))
      .catch((e) => onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"));
  };

  const readonlyRows: { label: string; value: string }[] = [
    { label: t("settings.portCloudcli", lang), value: String(READONLY.cloudcliPort) },
    { label: t("settings.portCaddy", lang), value: String(READONLY.caddyPort) },
    { label: t("settings.portDdnsgo", lang), value: String(READONLY.ddnsgoPort) },
    { label: t("settings.stackDir", lang), value: READONLY.stackDir },
    { label: t("settings.domain", lang), value: READONLY.domain },
  ];

  return (
    <>
      {/* 行为设置（AC21：五项开关） */}
      <section class="card">
        <h2 class="card__title">{t("settings.behavior", lang)}</h2>
        <div class="settings__rows">
          <SwitchRow
            label={t("settings.autostartServices", lang)}
            desc={t("settings.autostartServicesDesc", lang)}
            checked={settings.autostartServices}
            disabled={busyKey === "autostartServices"}
            onChange={(v) => void runAutostartToggle("autostartServices", v)}
          />
          <SwitchRow
            label={t("settings.autostartApp", lang)}
            desc={t("settings.autostartAppDesc", lang)}
            checked={settings.autostartApp}
            disabled={busyKey === "autostartApp"}
            onChange={(v) => void runAutostartToggle("autostartApp", v)}
          />
          <SwitchRow
            label={t("settings.linkStart", lang)}
            desc={t("settings.linkStartDesc", lang)}
            checked={settings.linkStartServices}
            onChange={(v) => void savePatch({ linkStartServices: v })}
          />
          <SwitchRow
            label={t("settings.openPageOnStart", lang)}
            desc={t("settings.openPageOnStartDesc", lang)}
            checked={settings.openPageOnStart}
            onChange={(v) => void savePatch({ openPageOnStart: v })}
          />
          <div class="settings__row">
            <div class="settings__row-text">
              <span class="settings__label">{t("settings.exitAction", lang)}</span>
            </div>
            <Segmented<ExitAction>
              value={settings.exitAction}
              options={[
                { value: "keep", label: t("settings.exitKeep", lang) },
                { value: "stop", label: t("settings.exitStop", lang) },
              ]}
              onChange={(v) => void savePatch({ exitAction: v })}
            />
          </div>
        </div>
      </section>

      {/* 穿透设置（spec 004 AC14/15/16） */}
      <section class="card">
        <h2 class="card__title">{t("settings.tunnel", lang)}</h2>
        <p class="muted">{t("settings.tunnelDesc", lang)}</p>
        <div class="tunnel-form-row">
          <div class="tunnel-form-field">
            <span class="settings__label">{t("settings.tunnelId", lang)}</span>
            <input
              class="form-input"
              placeholder={t("settings.tunnelIdPlaceholder", lang)}
              value={tunnelId}
              onInput={(e) => setTunnelId(e.currentTarget.value)}
            />
          </div>
          <div class="tunnel-form-field">
            <span class="settings__label">{t("settings.tunnelNodeDomain", lang)}</span>
            <input
              class="form-input"
              placeholder={t("settings.tunnelNodePlaceholder", lang)}
              value={nodeDomain}
              onInput={(e) => setNodeDomain(e.currentTarget.value)}
            />
          </div>
          <button class="btn btn--sm btn--primary tunnel-form-save" onClick={() => void saveTunnel()}>
            {t("settings.tunnelSave", lang)}
          </button>
        </div>
        <div class="settings__row">
          <div class="settings__row-text">
            <span class="settings__label">{t("settings.setFrpKeyHint", lang)}</span>
          </div>
          <button class="btn btn--sm" onClick={openSetFrpKey}>
            {t("settings.setFrpKey", lang)}
          </button>
        </div>
        <div class="settings__row">
          <div class="settings__row-text">
            <span class="settings__label">
              {t("settings.openStackDir", lang)}：
              <button class="link-btn" onClick={() => api.openStackDir().catch((e) => onToast(String(e), "error"))}>
                {READONLY.stackDir}
              </button>
            </span>
          </div>
        </div>
        <p class="muted">{t("settings.frpcDeploy", lang)}</p>
        <div class="settings__actions">
          <button
            class="btn btn--sm"
            onClick={() =>
              api
                .getDefenderExclusionCmd()
                .then(copyText)
                .then((ok) =>
                  onToast(
                    ok ? t("settings.whitelistCopied", lang) : t("toast.copyFailed", lang),
                    ok ? "success" : "error",
                  ),
                )
                .catch((e) => onToast(String(e), "error"))
            }
          >
            {t("settings.copyWhitelist", lang)}
          </button>
          <button
            class="btn btn--sm"
            onClick={() =>
              api
                .downloadFrpc()
                .then((msg) => onToast(`${t("settings.downloadFrpcDone", lang)}：${msg}`, "success"))
                .catch((e) => onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"))
            }
          >
            {t("settings.downloadFrpc", lang)}
          </button>
        </div>
      </section>

      {/* 域名心跳（spec 005 AC7） */}
      <section class="card">
        <h2 class="card__title">{t("settings.heartbeat", lang)}</h2>
        <SwitchRow
          label={t("settings.heartbeat", lang)}
          desc={t("settings.heartbeatDesc", lang)}
          checked={settings.domainHeartbeat}
          onChange={(v) => void savePatch({ domainHeartbeat: v })}
        />
      </section>

      {/* 语言（AC25：切换立即生效） */}
      <section class="card">
        <h2 class="card__title">{t("settings.language", lang)}</h2>
        <Segmented<LanguageSetting>
          value={settings.language}
          options={[
            { value: "auto", label: t("settings.langAuto", lang) },
            { value: "zh", label: t("settings.langZh", lang) },
            { value: "en", label: t("settings.langEn", lang) },
          ]}
          onChange={(v) => void switchLanguage(v)}
        />
      </section>

      {/* 只读卡（AC22：端口/路径/域名） */}
      <section class="card">
        <h2 class="card__title">{t("settings.readonly", lang)}</h2>
        <p class="muted">{t("settings.readonlyHint", lang)}</p>
        <div class="settings__rows">
          {readonlyRows.map((r) => (
            <div class="settings__row" key={r.label}>
              <div class="settings__row-text">
                <span class="settings__label">{r.label}</span>
                <code class="settings__value">{r.value}</code>
              </div>
              <CopyButton text={r.value} lang={lang} onToast={onToast} />
            </div>
          ))}
        </div>
        <div class="settings__actions">
          <button
            class="btn"
            onClick={() => api.openLogsDir().catch((e) => onToast(String(e), "error"))}
          >
            {t("settings.openLogs", lang)}
          </button>
        </div>
      </section>
    </>
  );
}

/** 开关行：标题 + 说明 + 视觉开关（styled checkbox，无依赖红线内） */
function SwitchRow(props: {
  label: string;
  desc: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label class={`switch-row${props.disabled ? " switch-row--disabled" : ""}`}>
      <span class="switch-row__body">
        <span class="settings__label">{props.label}</span>
        <span class="settings__desc">{props.desc}</span>
      </span>
      <input
        class="switch"
        type="checkbox"
        role="switch"
        checked={props.checked}
        disabled={props.disabled}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
      />
    </label>
  );
}

/** 分段单选（语言/退出行为共用） */
function Segmented<T extends string>(props: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div class="seg" role="group">
      {props.options.map((o) => (
        <button
          key={o.value}
          type="button"
          class={`seg__btn${props.value === o.value ? " seg__btn--active" : ""}`}
          onClick={() => props.onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
