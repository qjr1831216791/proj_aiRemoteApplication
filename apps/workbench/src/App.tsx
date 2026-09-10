/**
 * 应用壳（T13/T14 装配）：视图切换 + 全局状态装配 + toast。
 * - 窗口以 visible:false 创建，页面就绪后显形（规避 WebView2 首帧白屏，plan §3.1）；
 *   --hidden 启动（登录自启，AC10）时保持隐藏
 * - 状态数据流：启动 get_status 兜底 + `status://changed` 事件（前端不做轮询，AC4）
 * - 设置加载失败/损坏恢复（settings://repaired）→ 非阻塞 toast（AC24）
 * - openPageOnStart && 非隐藏启动 → 自动用浏览器打开工作台（AC22 行为项）
 * - 语言切换走 Rust set_language（托盘同源重建 + 脚本 -Lang 对齐，AC25），前端随之换词典
 */

import { useEffect, useState } from "preact/hooks";
import { api, onDomainHealth, onNetChanged, onSettingsRepaired, onStatusChanged, onTunnelStatus, onWizardChanged } from "./api";
import { detectLang, resolveLang, t, type Lang } from "./i18n";
import type {
  AccessUrls,
  ComponentId,
  ComponentStatus,
  DomainHealth,
  LanguageSetting,
  NetStatus,
  ScriptsAvailability,
  Settings,
  TunnelStatus,
  WizardStageId,
} from "./types";
import { MainView } from "./components/MainView";
import { SettingsView } from "./components/SettingsView";
import { WizardView } from "./components/WizardView";

type View = "main" | "settings" | "wizard";
type ToastKind = "info" | "success" | "error";
interface Toast {
  id: number;
  text: string;
  kind: ToastKind;
}

let toastSeq = 0;

export function App() {
  const [view, setView] = useState<View>("main");
  const [lang, setLang] = useState<Lang>(() => detectLang());
  const [settings, setSettings] = useState<Settings | null>(null);
  const [statuses, setStatuses] = useState<ComponentStatus[]>([]);
  const [urls, setUrls] = useState<AccessUrls | null>(null);
  const [scripts, setScripts] = useState<ScriptsAvailability | null>(null);
  const [netStatus, setNetStatus] = useState<NetStatus | null>(null);
  const [tunnelStatus, setTunnelStatus] = useState<TunnelStatus | null>(null);
  const [domainHealth, setDomainHealth] = useState<DomainHealth | null>(null);
  const [stopping, setStopping] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  // 装机向导（spec 006）：装机未完成 → 主看板引导条；跳转定位阶段（AC14）
  const [wizardFocus, setWizardFocus] = useState<WizardStageId | null>(null);
  const [wizardDone, setWizardDone] = useState(true);

  /** 非阻塞提示（自动 6s 消失） */
  const pushToast = (text: string, kind: ToastKind = "info") => {
    const id = ++toastSeq;
    setToasts((list) => [...list, { id, text, kind }]);
    setTimeout(() => setToasts((list) => list.filter((x) => x.id !== id)), 6000);
  };

  // ── 启动装配（一次性）─────────────────────────────────────────────────────
  useEffect(() => {
    if (!(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) return;
    const unsubs: Array<() => void> = [];
    let disposed = false;
    const track = (u: () => void) => {
      if (disposed) u();
      else unsubs.push(u);
    };
    void (async () => {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const hidden = await api.isHiddenStartup();
      if (!hidden) getCurrentWindow().show();

      try {
        const s = await api.getSettings();
        setSettings(s);
        setLang(resolveLang(s.language));
        if (s.openPageOnStart && !hidden) {
          api.openExternal("workbench").catch(() => {});
        }
      } catch (e) {
        pushToast(`${t("toast.loadFailed", lang)}: ${String(e)}`, "error");
      }

      api.getStatus().then(setStatuses).catch(() => {});
      api.getUrls().then(setUrls).catch(() => {});
      api.scriptsAvailability().then(setScripts).catch(() => {});
      api.getNetStatus().then(setNetStatus).catch(() => {});
      api.getTunnelStatus().then(setTunnelStatus).catch(() => {});
      api
        .wizardGetState()
        .then((s) => setWizardDone(s.done))
        .catch(() => {});

      // 状态事件：此后状态以事件为准（前端零轮询）
      track(await onStatusChanged(setStatuses));
      // 网络环境事件（spec 002）：变化才发（Rust 侧 15s 轮询去重）
      track(await onNetChanged(setNetStatus));
      // 隧道状态事件（spec 004）：守护线程 5s 收敛驱动，变化才发
      track(await onTunnelStatus(setTunnelStatus));
      // 域名心跳事件（spec 005）：60s 周期探测
      track(await onDomainHealth(setDomainHealth));
      // 设置损坏恢复：非阻塞提示（AC24）
      track(await onSettingsRepaired(() => pushToast(t("settings.repaired", lang), "info")));
      // 向导状态事件（spec 006）：引导条随 done 收敛
      track(await onWizardChanged((s) => setWizardDone(s.done)));
    })();
    return () => {
      disposed = true;
      unsubs.forEach((u) => u());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── 操作 ──────────────────────────────────────────────────────────────────
  const startAll = () =>
    api.startAll().catch((e) => pushToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"));

  /** 网络环境主动刷新（切换派发后加速收敛；spec 002） */
  const refreshNet = () => {
    api.getNetStatus().then(setNetStatus).catch(() => {});
  };

  const stopAll = async () => {
    setStopping(true);
    try {
      await api.stopAll();
    } catch (e) {
      pushToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setStopping(false);
    }
  };

  const retryOne = (id: ComponentId) =>
    api.startOne(id).catch((e) => pushToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"));

  /** 语言切换：Rust 侧持久化 + 托盘重建 + 脚本 -Lang 对齐（AC25 立即生效） */
  const changeLanguage = async (setting: LanguageSetting) => {
    try {
      const saved = await api.setLanguage(setting);
      setSettings(saved);
      setLang(resolveLang(saved.language));
    } catch (e) {
      pushToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    }
  };

  return (
    <main class="app">
      <header class="app__header">
        <div>
          <h1 class="app__title">{t("app.title", lang)}</h1>
          <p class="app__subtitle">{t("app.subtitle", lang)}</p>
        </div>
        <nav class="app__nav">
          <button
            class={`nav-btn${view === "main" ? " nav-btn--active" : ""}`}
            onClick={() => setView("main")}
          >
            {t("nav.main", lang)}
          </button>
          <button
            class={`nav-btn${view === "wizard" ? " nav-btn--active" : ""}`}
            onClick={() => {
              setWizardFocus(null);
              setView("wizard");
            }}
          >
            {t("nav.wizard", lang)}
          </button>
          <button
            class={`nav-btn${view === "settings" ? " nav-btn--active" : ""}`}
            onClick={() => setView("settings")}
          >
            {t("nav.settings", lang)}
          </button>
        </nav>
      </header>

      {view === "main" ? (
        <MainView
          lang={lang}
          statuses={statuses}
          urls={urls}
          scripts={scripts}
          netStatus={netStatus}
          onNetRefresh={refreshNet}
          settings={settings}
          tunnelStatus={tunnelStatus}
          domainHealth={domainHealth}
          onSettingsChange={setSettings}
          stopping={stopping}
          onStartAll={startAll}
          onStopAll={stopAll}
          onRetry={retryOne}
          onToast={pushToast}
          wizardDone={wizardDone}
          onOpenWizard={(stage) => {
            setWizardFocus(stage);
            setView("wizard");
          }}
        />
      ) : view === "wizard" && settings ? (
        <WizardView
          lang={lang}
          settings={settings}
          tunnelStatus={tunnelStatus}
          focusStage={wizardFocus}
          onToast={pushToast}
          onSettingsChange={setSettings}
          onFinished={() => setView("main")}
        />
      ) : settings ? (
        <SettingsView
          lang={lang}
          settings={settings}
          onToast={pushToast}
          onSettingsChange={setSettings}
          onLanguageChange={changeLanguage}
        />
      ) : (
        <section class="card">
          <p class="muted">{t("common.loading", lang)}</p>
        </section>
      )}

      <footer class="app__footer" />

      {/* 非阻塞提示栈 */}
      <div class="toasts">
        {toasts.map((x) => (
          <div key={x.id} class={`toast toast--${x.kind}`}>
            {x.text}
          </div>
        ))}
      </div>
    </main>
  );
}
