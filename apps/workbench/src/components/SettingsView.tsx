/**
 * 设置页（T14：AC21/22/24/25 展示层 + 007 T11：组网设置/服务卡）。
 * - 五项行为开关：两项自启开关先跑计划任务命令（tookOver → 提示）再持久化；
 *   联动补齐 / 退出行为 / 启动后打开页面为纯设置项，改即存
 * - 部署目录（spec 004）：编辑保存 + 打开栈目录（spec 008：随穿透卡退役迁入）
 * - 组网设置卡（007 AC11）：网络名/虚拟 IP/网段/对端节点编辑 + 前端预检
 *   （最终裁决在 Rust mesh_apply_config）；密钥只经脚本写入（无输入框，AC8）；
 *   服务管理（安装/应用/卸载，UAC 派发）
 * - 旧通道停用卡/穿透设置卡已随通道退役删除——spec 008（残留清理走
 *   uninstall-legacy.ps1，主看板 MeshCard 常驻入口承接日常维护）
 * - 语言：跟随系统/中文/英文三选，切换立即生效（App 负责 Rust 托盘重建 + 全局换词典）
 * - 端口/域名只读卡：一键复制 + "修改须重跑安装脚本"指引（AC22）
 * - 打开日志目录按钮；settings://repaired 事件提示在 App 层统一 toast
 * 全部乐观更新 + 失败回滚，长任务期间对应开关禁用防重复提交。
 */

import { useEffect, useRef, useState } from "preact/hooks";
import { api } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  ExitAction,
  LanguageSetting,
  MeshDiagItem,
  Settings,
  SettingsPatch,
} from "../types";
import { CopyButton } from "./CopyButton";

/**
 * 设置页分节锚点（左侧目录与滚动高亮共用一份清单）：id 与各 <section id> 一一对应，
 * 顺序即页面顺序——新增/调整卡片时两处同步。
 */
const SECTIONS: ReadonlyArray<{ id: string; key: DictKey }> = [
  { id: "settings-sec-behavior", key: "settings.behavior" },
  { id: "settings-sec-stackdir", key: "settings.stackDirEditable" },
  { id: "settings-sec-mesh", key: "settings.mesh" },
  { id: "settings-sec-auth", key: "settings.auth" },
  { id: "settings-sec-heartbeat", key: "settings.heartbeat" },
  { id: "settings-sec-language", key: "settings.language" },
  { id: "settings-sec-readonly", key: "settings.readonly" },
];

/** 目录高亮的判定线：视口顶部往下这么多像素，其上方最后一节即「当前节」 */
const TOC_LINE_PX = 120;

export interface SettingsViewProps {
  lang: Lang;
  settings: Settings;
  /** 生效域名（spec 013：后端 get_urls 下发解析；null = 尚未加载） */
  workbenchDomain: string | null;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
  /** 设置对象变更（乐观更新/回滚/服务端合并结果） */
  onSettingsChange: (s: Settings) => void;
  /** 语言切换（App：set_language → 托盘重建 + 前端全局换语言） */
  onLanguageChange: (setting: LanguageSetting) => Promise<void>;
}

// 只读展示常量：与 src-tauri/src/consts.rs 同步维护（改这里必须改那里，重跑安装脚本才生效）
// spec 013：域名行不再走本表——生效域名经 get_urls 下发（props.workbenchDomain）
const READONLY = {
  cloudcliPort: 3001,
  caddyPort: 443,
  stackDir: "D:\\Software\\cloudcli-https",
} as const;

export function SettingsView(props: SettingsViewProps) {
  const { lang, settings, workbenchDomain, onToast, onSettingsChange, onLanguageChange } = props;

  // ── 左侧目录（2026-09-13 需求方反馈）：点击平滑跳转 + 滚动高亮当前节 ──────
  const [activeSec, setActiveSec] = useState<string>(SECTIONS[0].id);
  const activeRef = useRef<string>(SECTIONS[0].id);

  useEffect(() => {
    let raf = 0;
    const compute = () => {
      raf = 0;
      const doc = document.documentElement;
      // 触底时直接落到最后一节——末节较短时其顶边可能永远到不了判定线
      const atBottom = window.scrollY + window.innerHeight >= doc.scrollHeight - 4;
      let cur = SECTIONS[0].id;
      if (atBottom) {
        cur = SECTIONS[SECTIONS.length - 1].id;
      } else {
        const line = window.scrollY + TOC_LINE_PX;
        for (const s of SECTIONS) {
          const el = document.getElementById(s.id);
          if (el && el.getBoundingClientRect().top + window.scrollY <= line) cur = s.id;
        }
      }
      // 只在真正换节时写状态，滚动过程不触发多余渲染
      if (cur !== activeRef.current) {
        activeRef.current = cur;
        setActiveSec(cur);
      }
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(compute);
    };
    compute();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    return () => {
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      if (raf) cancelAnimationFrame(raf);
    };
  }, []);

  /** 跳到某一节（平滑滚动；scroll-margin-top 已在 CSS 留出呼吸位） */
  const jumpTo = (id: string) => {
    activeRef.current = id;
    setActiveSec(id);
    document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  // 任务类开关在途标记（计划任务脚本最长 ~60s，期间禁用对应开关）
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [langBusy, setLangBusy] = useState(false);
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

  // 旧通道停用卡已随通道退役删除（spec 008）——残留清理走 uninstall-legacy.ps1

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

  // ── 访问账号（spec 011 T6/AC11：443 密码门——界面只发起与展示）─────────
  // 账号列表（null = 加载中）；哈希不出 Rust 侧，这里只有用户名。
  // 密码输入发生在派发的脚本窗口（Read-Host 不回显）——本界面无任何密码框。
  const [accounts, setAccounts] = useState<string[] | null>(null);
  const [authUser, setAuthUser] = useState("");
  const [authBusy, setAuthBusy] = useState(false);
  // 移除的两步确认（沿网络归类切换同款交互：行内确认，30s 未决自动还原）
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);
  useEffect(() => {
    if (confirmRemove === null) return;
    const id = setTimeout(() => setConfirmRemove(null), 30000);
    return () => clearTimeout(id);
  }, [confirmRemove]);

  const refreshAccounts = async () => {
    try {
      setAccounts(await api.httpsAuthList());
    } catch (e) {
      onToast(`${t("toast.loadFailed", lang)}: ${String(e)}`, "error");
    }
  };

  useEffect(() => {
    void refreshAccounts();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 派发 set-https-account.ps1（add/set/remove）：run_tool 只传非敏感参数，
   * 密码在弹出的控制台窗口输入；完成后由用户点「刷新」收敛列表 */
  const dispatchAuth = async (action: "add" | "set" | "remove", name: string) => {
    const user = name.trim();
    if (!/^[a-zA-Z0-9_-]{1,32}$/.test(user)) {
      onToast(t("settings.authNameInvalid", lang), "error");
      return;
    }
    setAuthBusy(true);
    setConfirmRemove(null);
    try {
      await api.runTool("set_https_account", {
        update: false,
        mirror: false,
        authAction: action,
        authUser: user,
      });
      onToast(t("settings.authDispatched", lang), "info");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setAuthBusy(false);
    }
  };

  /** 改密入口：从列表选中预填用户名（界面只收用户名，不收密码） */
  const startChangePassword = (user: string) => {
    setAuthUser(user);
    void dispatchAuth("set", user);
  };

  const readonlyRows: { label: string; value: string }[] = [
    { label: t("settings.portCloudcli", lang), value: String(READONLY.cloudcliPort) },
    { label: t("settings.portCaddy", lang), value: String(READONLY.caddyPort) },
    { label: t("settings.domain", lang), value: workbenchDomain ?? "" },
  ];

  return (
    <div class="settings-layout">
      {/* 左侧目录（2026-09-13 需求方反馈）：常驻跳转入口 + 当前节高亮。
          设置页 7 张卡很长，窄窗口下靠它一键到位 */}
      <nav class="settings-toc">
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            class={`settings-toc__item${
              activeSec === s.id ? " settings-toc__item--active" : ""
            }`}
            onClick={() => jumpTo(s.id)}
          >
            {t(s.key, lang)}
          </button>
        ))}
      </nav>

      <div class="settings-sections">
        {/* 行为设置（AC21：五项开关） */}
        <section class="card" id={SECTIONS[0].id}>
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
        <section class="card" id={SECTIONS[1].id}>
          <h2 class="card__title">{t("settings.stackDirEditable", lang)}</h2>
          <p class="muted">{t("settings.stackDirDesc", lang)}</p>
          <div class="settings__rows">
            {/* --field：行内宽字段（全仓仅此一处输入框落在 settings__row 里）——
                row-text 宽度按内容定，输入框的 width:100% 落不到实处，
                会退回浏览器固有宽约 170px，长路径看不全 */}
            <div class="settings__row settings__row--field">
              <div class="settings__row-text">
                <span class="settings__label">{t("settings.stackDir", lang)}</span>
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
              {t("settings.save", lang)}
            </button>
            <button
              class="btn btn--sm"
              onClick={() => api.openStackDir().catch((e) => onToast(String(e), "error"))}
            >
              {t("settings.openStackDir", lang)}
            </button>
          </div>
        </section>
  
        {/* 组网设置（spec 007 AC11）：非敏感配置编辑 + 密钥脚本入口（无输入框，AC8）+ 服务管理 */}
        <section class="card" id={SECTIONS[2].id}>
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
          {/* 保存配置独占一行（2026-09-13 需求方「不协调」反馈）：原先挤在字段行尾，
              与 textarea 右边界参差；移出后三输入平分整行、与 textarea 同宽 */}
          <div class="settings__actions mesh-save-row">
            <button class="btn btn--sm btn--primary" onClick={() => void saveMesh()}>
              {t("settings.save", lang)}
            </button>
          </div>
          <div class="settings__row mesh-start">
            <div class="settings__row-text">
              <span class="settings__desc">{t("settings.meshSecretHint", lang)}</span>
            </div>
            <button
              class="btn btn--sm"
              title={t("settings.meshSecretBtnTitle", lang)}
              onClick={openSetMeshSecret}
            >
              {t("settings.meshSecretBtn", lang)}
            </button>
          </div>
          <div class="settings__row mesh-start">
            <div class="settings__row-text">
              <span class="settings__label">{t("settings.meshServiceHint", lang)}</span>
            </div>
          </div>
          <div class="settings__actions mesh-actions">
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
          <div class="settings__row mesh-start">
            <div class="settings__row-text">
              <span class="settings__desc">{t("mesh.diag.desc", lang)}</span>
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
              {diag.map((d) => {
                // 文案约定：`<code>.bad` 写成「短标题：修复指引」——按首个全角冒号拆开，
                // 标题一行、指引与后端 detail 合并为灰字一行（原先整段指引挤在加粗标题里，
                // 四行粗体糊成一块；2026-09-13 需求方反馈「排版不协调」）
                const text = t(`mesh.diag.${d.code}.${d.ok ? "ok" : "bad"}` as DictKey, lang);
                const cut = text.indexOf("：");
                const title = cut > 0 ? text.slice(0, cut) : text;
                const trail = [cut > 0 ? text.slice(cut + 1) : "", d.detail ?? ""]
                  .filter((s) => s.length > 0)
                  .join(" · ");
                return (
                  <div class="settings__row" key={d.code}>
                    <div class="settings__row-text">
                      <span class="settings__label">
                        <span
                          class={d.ok ? "diag-mark diag-mark--ok" : "diag-mark diag-mark--bad"}
                        >
                          {d.ok ? "✓" : "✗"}
                        </span>{" "}
                        {title}
                      </span>
                      {trail ? <span class="settings__desc">{trail}</span> : null}
                    </div>
                  </div>
                );
              })}
            </div>
          ) : null}
        </section>
  
        {/* 访问账号（spec 011 T6/AC9/AC11：443 密码门——只发起与展示，
            密码在派发的脚本窗口输入，界面无密码框） */}
        <section class="card" id={SECTIONS[3].id}>
          {/* 标题行带「刷新」：它是"重读账号名单"的动作，归列表而非新增区
              （2026-09-13 需求方反馈：原先孤立在左下、与谁都不成组） */}
          <div class="card__head">
            <h2 class="card__title">{t("settings.auth", lang)}</h2>
            <button class="btn btn--sm" disabled={authBusy} onClick={() => void refreshAccounts()}>
              {t("settings.authRefreshBtn", lang)}
            </button>
          </div>
          <p class="muted">{t("settings.authDesc", lang)}</p>
          {accounts === null ? (
            <p class="muted">{t("common.loading", lang)}</p>
          ) : accounts.length === 0 ? (
            <p class="notice notice--warn">{t("settings.authEmptyHint", lang)}</p>
          ) : (
            <div class="settings__rows settings__rows--compact">
              {accounts.map((u) =>
                /* 确认态整行替换（2026-09-13 需求方反馈「看上去有点变形」）：
                   原先把确认块作为行的第二个 flex 子项与账号名并排——长文案换行后
                   行高被撑大，名字被 align-items:center 挤到行的垂直中心，
                   看起来像挂到了上一行身上。现在名字与动作都让位给确认文案，
                   所属账号由文案里的「账号名」点明 */
                confirmRemove === u ? (
                  <div class="settings__row" key={u}>
                    <div class="settings__row-text">
                      <span class="settings__desc">
                        {t("settings.authRemoveConfirm", lang).replace("{name}", u)}
                      </span>
                    </div>
                    <div class="settings__actions">
                      <button
                        class="btn btn--sm btn--danger"
                        disabled={authBusy}
                        onClick={() => void dispatchAuth("remove", u)}
                      >
                        {t("settings.authRemoveYes", lang)}
                      </button>
                      <button class="btn btn--sm" onClick={() => setConfirmRemove(null)}>
                        {t("common.cancel", lang)}
                      </button>
                    </div>
                  </div>
                ) : (
                  <div class="settings__row" key={u}>
                    <div class="settings__row-text">
                      <span class="settings__label">{u}</span>
                    </div>
                    <div class="settings__actions">
                      <button
                        class="btn btn--sm"
                        disabled={authBusy}
                        onClick={() => startChangePassword(u)}
                      >
                        {t("settings.authChangeBtn", lang)}
                      </button>
                      <button
                        class="btn btn--sm btn--danger"
                        disabled={authBusy}
                        onClick={() => setConfirmRemove(u)}
                      >
                        {t("settings.authRemoveBtn", lang)}
                      </button>
                    </div>
                  </div>
                ),
              )}
            </div>
          )}
          {/* 新增区（2026-09-13 需求方反馈重排）：与名单之间一条分隔线划清区块；
              输入框按既有 360px 上限而非撑满整行，按钮紧跟其后——原先 field 为
              flex:1 而 input 封顶 360，中间空出一大块、按钮被推到行尾 */}
          <div class="auth-add">
            <div class="tunnel-form-field">
              <span class="settings__label">{t("settings.authUsername", lang)}</span>
              <input
                class="form-input"
                placeholder={t("settings.authUserPlaceholder", lang)}
                value={authUser}
                onInput={(e) => setAuthUser(e.currentTarget.value)}
              />
            </div>
            <button
              class="btn btn--sm btn--primary tunnel-form-save"
              disabled={authBusy}
              title={t("settings.authAddHint", lang)}
              onClick={() => void dispatchAuth("add", authUser)}
            >
              {t("settings.authAddBtn", lang)}
            </button>
          </div>
        </section>
  
        {/* 域名心跳（spec 005 AC7） */}
        <section class="card" id={SECTIONS[4].id}>
          {/* 开关入标题行（2026-09-13 需求方反馈「有重复标题」）：本卡只有这一个
              设置项，原先行标签与卡片标题同文案、把名字说了两遍 */}
          <div class="card__head">
            <h2 class="card__title">{t("settings.heartbeat", lang)}</h2>
            <input
              class="switch"
              type="checkbox"
              role="switch"
              aria-label={t("settings.heartbeat", lang)}
              checked={settings.domainHeartbeat}
              onChange={(e) => void savePatch({ domainHeartbeat: e.currentTarget.checked })}
            />
          </div>
          <p class="muted">{t("settings.heartbeatDesc", lang)}</p>
        </section>
  
        {/* 语言（AC25：切换立即生效） */}
        <section class="card" id={SECTIONS[5].id}>
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
        <section class="card" id={SECTIONS[6].id}>
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
            {/* 日志目录并入同一份只读行列表（2026-09-13 需求方反馈「单独一个按钮不太协调」）：
                标签在左、动作在右，按钮尺寸与之上各行的 CopyButton 一致；
                不再在卡片底部单挂一个按钮行 */}
            <div class="settings__row">
              <div class="settings__row-text">
                <span class="settings__label">{t("settings.logsDir", lang)}</span>
              </div>
              <button
                class="btn btn--sm"
                onClick={() => api.openLogsDir().catch((e) => onToast(String(e), "error"))}
              >
                {t("common.open", lang)}
              </button>
            </div>
          </div>
        </section>
      </div>
    </div>
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
