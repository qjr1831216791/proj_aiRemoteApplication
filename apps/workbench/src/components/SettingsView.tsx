/**
 * 设置页（T14：AC21/22/24/25 展示层 + 007 T11：组网设置/服务/停用卡）。
 * - 五项行为开关：两项自启开关先跑计划任务命令（tookOver → 提示）再持久化；
 *   联动补齐 / 退出行为 / 启动后打开页面为纯设置项，改即存
 * - 组网设置卡（007 AC11）：网络名/虚拟 IP/网段/对端节点编辑 + 前端预检
 *   （最终裁决在 Rust mesh_apply_config）；密钥只经脚本写入（无输入框，AC8）；
 *   服务管理（安装/应用/卸载，UAC 派发）
 * - 旧通道停用卡（007 AC5/AC6）：非现役方可停用；直连停用时可选删 A 记录
 *   （仅穿透现役时提供——mesh 态 A=虚拟 IP 不删）；停用穿透后引导清 SAKURA_FRP_KEY
 * - 语言：跟随系统/中文/英文三选，切换立即生效（App 负责 Rust 托盘重建 + 全局换词典）
 * - 端口/路径/域名只读卡：一键复制 + "修改须重跑安装脚本"指引（AC22）
 * - 打开日志目录按钮；settings://repaired 事件提示在 App 层统一 toast
 * 全部乐观更新 + 失败回滚，长任务期间对应开关禁用防重复提交。
 */

import { useEffect, useState } from "preact/hooks";
import { api, copyText } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  ExitAction,
  LanguageSetting,
  MeshDiagItem,
  Settings,
  SettingsPatch,
} from "../types";
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
  // 部署目录（spec 004：用户输入安装根，重启生效）
  const [stackDir, setStackDir] = useState(settings.stackDir);

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

  /** 部署目录保存（spec 004）：非空校验 → 持久化；重启工作台后全链生效 */
  const saveStackDir = async () => {
    const dir = stackDir.trim();
    if (!dir) {
      onToast(t("settings.stackDirEmpty", lang), "error");
      return;
    }
    try {
      const saved = await api.saveSettings({ stackDir: dir });
      onSettingsChange(saved);
      setStackDir(saved.stackDir);
      onToast(t("settings.stackDirSaved", lang), "success");
    } catch (e) {
      onToast(`${t("toast.saveFailed", lang)}: ${String(e)}`, "error");
    }
  };

  // 组网设置表单（007 AC11；非敏感部分，密钥走脚本通道）
  const [meshName, setMeshName] = useState(settings.mesh.networkName);
  const [meshIp, setMeshIp] = useState(settings.mesh.virtualIp);
  const [meshCidr, setMeshCidr] = useState(settings.mesh.virtualCidr);
  const [meshPeersText, setMeshPeersText] = useState(settings.mesh.peers.join("\n"));
  // 组网动作在途（安装/应用/卸载共用；UAC 派发为异步返回）
  const [meshBusy, setMeshBusy] = useState(false);
  // 组网诊断结果（007 T16/AC13；null = 尚未运行）
  const [diag, setDiag] = useState<MeshDiagItem[] | null>(null);
  const [diagBusy, setDiagBusy] = useState(false);

  /** 组网配置保存（AC11）：前端预检（与 Rust validate_mesh_config 同形宽松，
   * 最终裁决在 mesh_apply_config）→ 持久化 */
  const saveMesh = async () => {
    const name = meshName.trim();
    const ip = meshIp.trim();
    const cidr = meshCidr.trim();
    const peers = meshPeersText
      .split("\n")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (!name) {
      onToast(t("settings.meshNameRequired", lang), "error");
      return;
    }
    const ipL = ipv4ToLong(ip);
    if (ipL === null) {
      onToast(t("settings.meshIpInvalid", lang), "error");
      return;
    }
    const m = /^(\d{1,3}(?:\.\d{1,3}){3})\/(\d{1,2})$/.exec(cidr);
    const netL = m ? ipv4ToLong(m[1]) : null;
    const prefix = m ? Number(m[2]) : -1;
    if (netL === null || prefix < 0 || prefix > 32) {
      onToast(t("settings.meshCidrInvalid", lang), "error");
      return;
    }
    const mask = prefix === 0 ? 0 : (0xffffffff << (32 - prefix)) >>> 0;
    if ((((ipL ^ netL) as number) & mask) !== 0) {
      onToast(t("settings.meshIpNotInCidr", lang), "error");
      return;
    }
    if (peers.length === 0 || peers.some((p) => !/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\/\S+$/.test(p))) {
      onToast(t("settings.meshPeersInvalid", lang), "error");
      return;
    }
    try {
      onSettingsChange(
        await api.saveSettings({
          mesh: { networkName: name, virtualIp: ip, virtualCidr: cidr, peers },
        }),
      );
      onToast(t("settings.meshSaved", lang), "success");
    } catch (e) {
      onToast(`${t("toast.saveFailed", lang)}: ${String(e)}`, "error");
    }
  };

  /** 组网服务动作统一派发（安装/应用/卸载；UAC 通过后状态由 mesh://status 收敛） */
  const runMeshAction = async (
    kind: "install" | "apply" | "uninstall",
    doneKey: Parameters<typeof t>[0],
  ) => {
    setMeshBusy(true);
    try {
      if (kind === "install") await api.meshInstallService();
      else if (kind === "apply") await api.meshApplyConfig();
      else await api.meshUninstallService();
      onToast(t(doneKey, lang), "info");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setMeshBusy(false);
    }
  };

  // 旧通道停用确认态（007 AC5/AC6：两步确认，30s 自动还原沿全局确认模式）
  const [confirmDisable, setConfirmDisable] = useState<"tunnel" | "direct" | null>(null);
  const [deleteA, setDeleteA] = useState(false);
  const [disabling, setDisabling] = useState(false);

  /** 停用旧通道（AC5/AC6）：Rust 侧前置校验（现役不可停）→ 设置回写 */
  const doDisable = async (target: "tunnel" | "direct") => {
    setConfirmDisable(null);
    setDisabling(true);
    try {
      onSettingsChange(await api.disableLegacyChannel(target, deleteA));
      onToast(t(target === "tunnel" ? "channel.disabled.doneTunnel" : "channel.disabled.doneDirect", lang), "success");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setDisabling(false);
      setDeleteA(false);
    }
  };

  // 停用确认 30s 未确认自动还原（给足阅读风险文案时间，沿全局确认模式）
  useEffect(() => {
    if (confirmDisable === null) return;
    const id = setTimeout(() => setConfirmDisable(null), 30_000);
    return () => clearTimeout(id);
  }, [confirmDisable]);

  /** 组网密钥脚本入口（AC8/AC11：拉起控制台交互窗，密钥经脚本直写栈目录
   * network-secret 文件，不进 IPC 载荷/设置文件/日志——前端无密钥输入框） */
  const openSetMeshSecret = () => {
    api
      .runTool("set_mesh_secret", { update: false, mirror: false })
      .then(() => onToast(t("settings.meshSecretDispatched", lang), "info"))
      .catch((e) => onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"));
  };

  /** 组网诊断（007 T16/AC13）：六项只读探测（服务/密钥/对端可达/成员/本机
   * 网卡/域名链路），Rust 侧 spawn_blocking，逐对端 3s 超时可能耗时数秒 */
  const runDiagnostics = async () => {
    setDiagBusy(true);
    try {
      setDiag(await api.meshDiagnostics());
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setDiagBusy(false);
    }
  };

  /** 清除 SakuraFrp 访问密钥（停用穿透收尾，AC5）：拉起 clear-frp-key.ps1 */
  const runClearFrpKey = () => {
    api
      .clearFrpKey()
      .then(() => onToast(t("channel.disabled.clearKeyDone", lang), "info"))
      .catch((e) => onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"));
  };

  const readonlyRows: { label: string; value: string }[] = [
    { label: t("settings.portCloudcli", lang), value: String(READONLY.cloudcliPort) },
    { label: t("settings.portCaddy", lang), value: String(READONLY.caddyPort) },
    { label: t("settings.portDdnsgo", lang), value: String(READONLY.ddnsgoPort) },
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

      {/* 部署目录（spec 004：用户可配置，重启生效） */}
      <section class="card">
        <h2 class="card__title">{t("settings.stackDirEditable", lang)}</h2>
        <p class="muted">{t("settings.stackDirDesc", lang)}</p>
        <div class="settings__rows">
          <div class="settings__row">
            <div class="settings__row-text">
              <span class="settings__label">{t("settings.stackDirEditable", lang)}</span>
              <input
                class="form-input"
                value={stackDir}
                onInput={(e) => setStackDir(e.currentTarget.value)}
              />
            </div>
          </div>
        </div>
        <div class="settings__actions">
          <button class="btn btn--sm btn--primary" onClick={() => void saveStackDir()}>
            {t("settings.tunnelSave", lang)}
          </button>
        </div>
      </section>

      {/* 组网设置（spec 007 AC11）：非敏感配置编辑 + 密钥脚本入口（无输入框，AC8）+ 服务管理 */}
      <section class="card">
        <h2 class="card__title">{t("settings.mesh", lang)}</h2>
        <p class="muted">{t("settings.meshDesc", lang)}</p>
        <div class="tunnel-form-row">
          <div class="tunnel-form-field">
            <span class="settings__label">{t("settings.meshName", lang)}</span>
            <input
              class="form-input"
              value={meshName}
              onInput={(e) => setMeshName(e.currentTarget.value)}
            />
          </div>
          <div class="tunnel-form-field">
            <span class="settings__label">{t("settings.meshIp", lang)}</span>
            <input
              class="form-input"
              placeholder={t("settings.meshIpPlaceholder", lang)}
              value={meshIp}
              onInput={(e) => setMeshIp(e.currentTarget.value)}
            />
          </div>
          <div class="tunnel-form-field">
            <span class="settings__label">{t("settings.meshCidr", lang)}</span>
            <input
              class="form-input"
              value={meshCidr}
              onInput={(e) => setMeshCidr(e.currentTarget.value)}
            />
          </div>
          <button
            class="btn btn--sm btn--primary tunnel-form-save"
            onClick={() => void saveMesh()}
          >
            {t("settings.tunnelSave", lang)}
          </button>
        </div>
        <div class="tunnel-form-field">
          <span class="settings__label">{t("settings.meshPeers", lang)}</span>
          <textarea
            class="form-input"
            rows={3}
            placeholder={t("settings.meshPeersPlaceholder", lang)}
            value={meshPeersText}
            onInput={(e) => setMeshPeersText(e.currentTarget.value)}
          />
        </div>
        <div class="settings__row">
          <div class="settings__row-text">
            <span class="settings__label">{t("settings.meshSecretHint", lang)}</span>
          </div>
          <button class="btn btn--sm" onClick={openSetMeshSecret}>
            {t("settings.meshSecretBtn", lang)}
          </button>
        </div>
        <div class="settings__row">
          <div class="settings__row-text">
            <span class="settings__label">{t("settings.meshServiceHint", lang)}</span>
          </div>
        </div>
        <div class="settings__actions">
          <button
            class="btn btn--sm"
            disabled={meshBusy}
            onClick={() => void runMeshAction("install", "settings.meshInstallDone")}
          >
            {t("settings.meshInstallBtn", lang)}
          </button>
          <button
            class="btn btn--sm"
            disabled={meshBusy}
            onClick={() => void runMeshAction("apply", "settings.meshApplyDone")}
          >
            {t("settings.meshApplyBtn", lang)}
          </button>
          <button
            class="btn btn--sm btn--danger"
            disabled={meshBusy}
            onClick={() => void runMeshAction("uninstall", "settings.meshUninstallDone")}
          >
            {t("settings.meshUninstallBtn", lang)}
          </button>
        </div>
        {/* 组网诊断（007 T16/AC13）：六项只读探测，成员访问异常时自查断点 */}
        <div class="settings__row">
          <div class="settings__row-text">
            <span class="settings__label">{t("mesh.diag.desc", lang)}</span>
          </div>
          <button class="btn btn--sm" disabled={diagBusy} onClick={() => void runDiagnostics()}>
            {diagBusy ? t("mesh.diag.running", lang) : t("mesh.diag.runBtn", lang)}
          </button>
        </div>
        {diag ? (
          <div class="settings__rows">
            {(() => {
              const failed = diag.filter((d) => !d.ok).length;
              return failed === 0 ? (
                <p class="notice notice--ok">{t("mesh.diag.summaryOk", lang)}</p>
              ) : (
                <p class="notice notice--warn">
                  {t("mesh.diag.summaryBad", lang).replace("{n}", String(failed))}
                </p>
              );
            })()}
            {diag.map((d) => (
              <div class="settings__row" key={d.code}>
                <div class="settings__row-text">
                  <span class="settings__label">
                    {d.ok ? "✓" : "✗"}{" "}
                    {t(`mesh.diag.${d.code}.${d.ok ? "ok" : "bad"}` as DictKey, lang)}
                  </span>
                  {d.detail ? <span class="muted">{d.detail}</span> : null}
                </div>
              </div>
            ))}
          </div>
        ) : null}
      </section>

      {/* 旧通道停用（spec 007 AC5/AC6）：非现役方可停用；停用穿透后引导清访问密钥 */}
      <section class="card">
        <h2 class="card__title">{t("channel.disabled.title", lang)}</h2>
        <p class="muted">{t("channel.disabled.desc", lang)}</p>
        <div class="settings__rows">
          {(["tunnel", "direct"] as const).map((target) => {
            const disabled =
              target === "tunnel" ? settings.tunnelDisabled : settings.directDisabled;
            const active = settings.accessChannel === target;
            return (
              <div key={target}>
                <div class="settings__row">
                  <div class="settings__row-text">
                    <span class="settings__label">
                      {t(
                        target === "tunnel"
                          ? "channel.disabled.tunnelName"
                          : "channel.disabled.directName",
                        lang,
                      )}
                    </span>
                    <span
                      class={`chip ${active ? "chip--running" : disabled ? "chip--stopped" : "chip--net-domain"}`}
                    >
                      {active
                        ? t("channel.disabled.activeNow", lang)
                        : disabled
                          ? t("channel.disabled.disabledBadge", lang)
                          : t("channel.disabled.idleBadge", lang)}
                    </span>
                  </div>
                  {active ? (
                    <span class="muted">{t("channel.disabled.activeHint", lang)}</span>
                  ) : disabled ? (
                    <span class="muted">{t("channel.disabled.reenableHint", lang)}</span>
                  ) : (
                    <button
                      class="btn btn--sm"
                      disabled={disabling}
                      onClick={() => setConfirmDisable(target)}
                    >
                      {t(
                        target === "tunnel"
                          ? "channel.disabled.disableTunnel"
                          : "channel.disabled.disableDirect",
                        lang,
                      )}
                    </button>
                  )}
                </div>
                {confirmDisable === target ? (
                  <div class="net__confirm">
                    <p class="net__risk">
                      {t(
                        target === "tunnel"
                          ? "channel.disabled.confirmTunnel"
                          : "channel.disabled.confirmDirect",
                        lang,
                      )}
                    </p>
                    {target === "direct" && settings.accessChannel === "tunnel" ? (
                      <label class="switch-row">
                        <span class="switch-row__body">
                          <span class="settings__label">
                            {t("channel.disabled.deleteA", lang)}
                          </span>
                        </span>
                        <input
                          class="switch"
                          type="checkbox"
                          role="switch"
                          checked={deleteA}
                          onChange={(e) => setDeleteA(e.currentTarget.checked)}
                        />
                      </label>
                    ) : null}
                    <button
                      class="btn btn--sm btn--danger"
                      disabled={disabling}
                      onClick={() => void doDisable(target)}
                    >
                      {t("tunnel.confirm", lang)}
                    </button>
                    <button
                      class="btn btn--sm"
                      disabled={disabling}
                      onClick={() => setConfirmDisable(null)}
                    >
                      {t("tunnel.cancel", lang)}
                    </button>
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
        {settings.tunnelDisabled ? (
          <div class="settings__row">
            <div class="settings__row-text">
              <span class="settings__label">{t("channel.disabled.clearKeyHint", lang)}</span>
            </div>
            <button class="btn btn--sm btn--danger" onClick={runClearFrpKey}>
              {t("channel.disabled.clearKeyBtn", lang)}
            </button>
          </div>
        ) : null}
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
                {settings.stackDir}
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

/** IPv4 → 无符号 32 位数（组网预检用；非法返回 null） */
function ipv4ToLong(ip: string): number | null {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(ip);
  if (!m) return null;
  const parts = m.slice(1).map(Number);
  if (parts.some((p) => p > 255)) return null;
  return (((parts[0] << 24) | (parts[1] << 16) | (parts[2] << 8) | parts[3]) as number) >>> 0;
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
