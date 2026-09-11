/**
 * 主界面（T13：AC1/4/6 展示层；spec 008 二元化）。
 * - 总开关：启动/停止双按钮，进行中禁用防重复；部分失败不显示笼统"启动成功"，
 *   实况由各组件状态卡自述（AC6）
 * - 两组件状态卡：五态色、端口、当前态耗时（since）、失败/port-held 原因、
 *   组件级重试（AC6）
 * - 地址区：本机/局域网/域名三行，一键复制 + 打开（AC19 地址区）
 * 状态数据流：status://changed 事件（App 订阅）+ 启动时 get_status 兜底。
 */

import { useEffect, useState } from "preact/hooks";
import { api } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  AccessUrls,
  ComponentId,
  ComponentState,
  ComponentStatus,
  DomainHealth,
  MeshStatus,
  NetCategory,
  NetStatus,
  ScriptsAvailability,
  Settings,
  WizardStageId,
} from "../types";
import { CopyButton } from "./CopyButton";
import { MeshCard } from "./MeshCard";
import { ToolsSection } from "./ToolsSection";

export interface MainViewProps {
  lang: Lang;
  statuses: ComponentStatus[];
  urls: AccessUrls | null;
  scripts: ScriptsAvailability | null;
  /** 网络环境快照（spec 002；null = 尚无成功探测） */
  netStatus: NetStatus | null;
  /** 主动刷新网络环境（切换派发成功后加速收敛，免等 15s 轮询） */
  onNetRefresh: () => void;
  /** 全量设置（组网卡数据源；App 持有） */
  settings: Settings | null;
  /** 组网运行状态（spec 007；null = 尚无快照） */
  meshStatus: MeshStatus | null;
  /** 域名心跳快照（spec 005；null = 尚无探测结果） */
  domainHealth: DomainHealth | null;
  /** 一键停止在途（防重复点击） */
  stopping: boolean;
  /** 装机向导是否已完成（spec 006 AC1：未完成 → 引导条） */
  wizardDone: boolean;
  /** 跳转装机向导并定位阶段（spec 006 AC14） */
  onOpenWizard: (stage: WizardStageId) => void;
  onStartAll: () => void;
  onStopAll: () => void;
  onRetry: (id: ComponentId) => void;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
}

export function MainView(props: MainViewProps) {
  const {
    lang, statuses, urls, scripts, netStatus, onNetRefresh,
    settings, meshStatus, domainHealth,
    stopping, onStartAll, onStopAll, onRetry, onToast,
    wizardDone, onOpenWizard,
  } = props;
  // 当前态耗时（since → now）每秒刷新
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  // 网络归类切换的两步确认（需求方定：30s 未确认自动还原，给足阅读风险文案时间）
  const [confirmIf, setConfirmIf] = useState<number | null>(null);
  const [confirmCat, setConfirmCat] = useState<"private" | "public" | null>(null);
  useEffect(() => {
    if (confirmIf === null) return;
    const id = setTimeout(() => setConfirmIf(null), 30000);
    return () => clearTimeout(id);
  }, [confirmIf]);

  const switchNet = (name: string, ifIndex: number, category: "private" | "public") => {
    setConfirmIf(null);
    api
      .setNetworkCategory(name, ifIndex, category)
      .then(() => {
        onToast(t("net.dispatched", lang), "success");
        // UAC 批准后给执行留几秒，主动拉取加速收敛（免干等 15s 轮询）
        setTimeout(() => onNetRefresh(), 3500);
      })
      .catch((e) => onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"));
  };

  const starting = statuses.some((s) => s.state === "starting");
  const anyRunning = statuses.some((s) => s.state === "running");
  // 两组件全部运行中：启动无事可做，禁用并明示（避免"点了没反应"的静默无操作）
  const allRunning = statuses.length > 0 && statuses.every((s) => s.state === "running");
  const busy = starting || stopping;

  return (
    <>
      {/* 装机引导条（spec 006 AC1）：装机未完成时醒目入口，可关闭由向导 done 收敛 */}
      {!wizardDone ? (
        <section class="card master">
          <p class="notice notice--warn">{t("wizard.notice", lang)}</p>
          <div class="master__actions">
            <button class="btn btn--primary" onClick={() => onOpenWizard("basis")}>
              {t("wizard.noticeCta", lang)}
            </button>
          </div>
        </section>
      ) : null}

      {/* 总开关（spec §4.6 信息分区之首） */}
      <section class="card master">
        <div class="master__actions">
          <button
            class="btn btn--primary"
            disabled={busy || allRunning}
            onClick={onStartAll}
          >
            {allRunning ? t("main.allRunning", lang) : t("main.startAll", lang)}
          </button>
          <button class="btn btn--danger" disabled={busy || !anyRunning} onClick={onStopAll}>
            {t("main.stopAll", lang)}
          </button>
        </div>
        <p class="master__hint">
          {busy
            ? t("main.busy", lang)
            : allRunning
              ? t("main.allRunningHint", lang)
              : t("main.stopHint", lang)}
        </p>
      </section>

      {/* 两组件状态卡 */}
      <section class="status-grid">
        {statuses.map((s) => (
          <article key={s.id} class={`card status status--${s.state}`}>
            <header class="status__head">
              <span class="status__name">{t(`component.${s.id}`, lang)}</span>
              <span class={`chip chip--${s.state}`}>{stateLabel(s.state, lang)}</span>
            </header>
            <p class="status__meta">
              {t("main.port", lang)} {s.port}
              <span class="status__sep">·</span>
              {t("main.elapsed", lang)} {fmtElapsed(now - s.since)}
              {s.state === "starting" ? (
                <>
                  <span class="status__sep">·</span>
                  {t("main.startingWait", lang)}
                </>
              ) : null}
            </p>
            {s.detail ? <p class="status__detail">{s.detail}</p> : null}
            {s.state === "failed" || s.state === "port-held" ? (
              <button class="btn btn--sm" disabled={busy} onClick={() => onRetry(s.id)}>
                {t("common.retry", lang)}
              </button>
            ) : null}
            {s.state === "failed" && s.id === "cloudcli" ? (
              <button class="btn btn--sm" onClick={() => onOpenWizard("basis")}>
                {t("main.goInstall", lang)}
              </button>
            ) : null}
          </article>
        ))}
      </section>

      {/* 网络环境（spec 002）：被拦截反馈 + 用户决策的归类调整 */}
      <section class="card">
        <h2 class="card__title">{t("net.title", lang)}</h2>
        {netStatus?.alert ? (
          <p class="notice notice--warn">
            {t("net.alert", lang).replace(
              "{names}",
              netStatus.networks
                .filter((n) => n.category === "public")
                .map((n) => n.name)
                .join("、"),
            )}
          </p>
        ) : null}
        {netStatus === null || netStatus.networks.length === 0 ? (
          <p class="muted">{t("net.noNetworks", lang)}</p>
        ) : (
          netStatus.networks.map((n) => (
            <div key={n.ifIndex}>
              <div class="net__row">
                <span class="net__name">{n.name}</span>
                <span class={netChipClass(n.category)}>{t(netCatKey(n.category), lang)}</span>
                <span class="net__spacer" />
                {n.category === "public" ? (
                  <button
                    class="btn btn--sm"
                    onClick={() => {
                      setConfirmIf(n.ifIndex);
                      setConfirmCat("private");
                    }}
                  >
                    {t("net.setPrivate", lang)}
                  </button>
                ) : n.category === "private" ? (
                  <button
                    class="btn btn--sm"
                    onClick={() => {
                      setConfirmIf(n.ifIndex);
                      setConfirmCat("public");
                    }}
                  >
                    {t("net.setPublic", lang)}
                  </button>
                ) : null}
              </div>
              {confirmIf === n.ifIndex && confirmCat ? (
                <div class="net__confirm">
                  <p class="net__risk">
                    {confirmCat === "private" ? t("net.riskPrivate", lang) : t("net.riskPublic", lang)}
                  </p>
                  <button
                    class="btn btn--sm btn--primary"
                    onClick={() => switchNet(n.name, n.ifIndex, confirmCat)}
                  >
                    {confirmCat === "private" ? t("net.confirmPrivate", lang) : t("net.confirmPublic", lang)}
                  </button>
                  <button class="btn btn--sm" onClick={() => setConfirmIf(null)}>
                    {t("net.cancel", lang)}
                  </button>
                </div>
              ) : null}
            </div>
          ))
        )}
      </section>

      {/* 访问通道（spec 008：组网单通道）：组网状态 + 虚拟 IP/成员 + DNS 指引 */}
      <MeshCard lang={lang} settings={settings} meshStatus={meshStatus} onToast={onToast} />

      {/* 地址区 */}
      <section class="card">
        <h2 class="card__title">{t("addr.title", lang)}</h2>
        {urls
          ? (["local", "lan", "domain"] as const).map((k) => (
              <div class="addr__row" key={k}>
                <span class="addr__label">
                  {k === "domain" && domainHealth ? (
                    <span
                      class={`hb-dot ${domainHealth.healthy ? "hb-dot--ok" : "hb-dot--fail"}`}
                      title={`${t(`heartbeat.kind.${domainHealth.kind}`, lang).replace("{code}", String(domainHealth.code ?? ""))} · ${t("heartbeat.scopeNote", lang)}`}
                    />
                  ) : null}
                  {t(`addr.${k}`, lang)}
                </span>
                <code class="addr__url">{urls[k]}</code>
                <span class="addr__actions">
                  <CopyButton text={urls[k]} lang={lang} onToast={onToast} />
                  <button
                    class="btn btn--sm"
                    onClick={() =>
                      api
                        .openExternal(k === "local" ? "local" : k === "lan" ? "lan" : "domain")
                        .catch((e) => onToast(String(e), "error"))
                    }
                  >
                    {t("common.open", lang)}
                  </button>
                </span>
              </div>
            ))
          : null}
      </section>

      {/* 低频操作区（折叠） */}
      <ToolsSection lang={lang} scripts={scripts} onToast={onToast} />
    </>
  );
}

/** 五态 → 词典标签 */
function stateLabel(state: ComponentState, lang: Lang): string {
  const key: DictKey = state === "port-held" ? "common.portHeld" : `common.${state}`;
  return t(key, lang);
}

/** 网络归类 → chip 配色（公用橙警 / 专用绿 / 域与未知灰） */
function netChipClass(category: NetCategory): string {
  switch (category) {
    case "public":
      return "chip chip--net-public";
    case "private":
      return "chip chip--net-private";
    default:
      return "chip chip--net-domain";
  }
}

/** 网络归类 → 词典标签 */
function netCatKey(category: NetCategory): DictKey {
  switch (category) {
    case "public":
      return "net.catPublic";
    case "private":
      return "net.catPrivate";
    case "domain":
      return "net.catDomain";
    default:
      return "net.catUnknown";
  }
}

/** 耗时格式化：48s / 3m24s / 1h05m（语言无关） */
export function fmtElapsed(ms: number): string {
  const secs = Math.max(0, Math.floor(ms / 1000));
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m${String(secs % 60).padStart(2, "0")}s`;
  const hours = Math.floor(mins / 60);
  return `${hours}h${String(mins % 60).padStart(2, "0")}m`;
}
