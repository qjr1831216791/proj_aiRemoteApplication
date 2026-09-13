/**
 * 主界面（T13：AC1/4/6 展示层；spec 008 二元化）。
 * - 总开关：启动/停止双按钮，进行中禁用防重复；部分失败不显示笼统"启动成功"，
 *   实况由各组件状态卡自述（AC6）
 * - 两组件状态卡：五态色、端口、当前态耗时（since）、失败/port-held 原因、
 *   组件级重试（AC6）
 * - 地址区：本机/局域网/域名三行，一键复制 + 打开（AC19 地址区）；
 *   局域网行按白名单健康三态如实措辞（spec 010 AC5/AC8）+ 前置绿/红二值豆
 *   （验收期第 4 项：判定纯函数 lanDot.ts，例外规则在 ∧ 归类专用 → 绿）
 * 状态数据流：status://changed 事件（App 订阅）+ 启动时 get_status 兜底；
 * 白名单健康（spec 010）：languard://changed 事件 + 启动 lan_guard_status 兜底。
 */

import { useEffect, useState } from "preact/hooks";
import { api, onLanGuardChanged } from "../api";
import { lanDotOk } from "../lanDot";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  AccessUrls,
  ComponentId,
  ComponentState,
  ComponentStatus,
  DomainHealth,
  ExceptionState,
  LanHealth,
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
  /** 主动刷新网络环境（切换派发成功后加速收敛，免等 60s 轮询） */
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

  // 白名单健康（spec 010）：启动探测兜底 + languard://changed 事件驱动
  //（后端 60s 监视轮询 + 动作后即时刷新，变化才发声）；地址区与组网卡共用
  const [lanHealth, setLanHealth] = useState<LanHealth | null>(null);
  useEffect(() => {
    let disposed = false;
    api
      .lanGuardStatus()
      .then((h) => {
        if (!disposed) setLanHealth(h);
      })
      .catch(() => {});
    const unsub = onLanGuardChanged(setLanHealth);
    return () => {
      disposed = true;
      unsub.then((u) => u());
    };
  }, []);
  /** 白名单动作后即时复测（UAC 窗内规则数秒后落位，另由 MeshCard 延迟追加） */
  const refreshLan = () => {
    api.lanGuardStatus().then(setLanHealth).catch(() => {});
  };

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
        // UAC 批准后给执行留几秒，主动拉取加速收敛（免干等 60s 轮询）
        setTimeout(() => onNetRefresh(), 3500);
      })
      .catch((e) => onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"));
  };

  const starting = statuses.some((s) => s.state === "starting");
  const anyRunning = statuses.some((s) => s.state === "running");
  // 两组件全部运行中：启动无事可做，禁用并明示（避免"点了没反应"的静默无操作）
  const allRunning = statuses.length > 0 && statuses.every((s) => s.state === "running");
  const busy = starting || stopping;

  // 局域网行绿豆（spec 010 验收期第 4 项）：绿 = 例外规则实况存在（on/expired）
  // ∧ 活动网络归类专用；红 = 其余（off/pending/无数据/含公用）。判定纯函数在
  // lanDot.ts（附单测）；网络归类数据沿网络卡同源（netStatus 轮询 + net://changed）
  const lanDotGreen = lanDotOk(
    lanHealth?.exception ?? null,
    netStatus?.networks.map((n) => n.category) ?? [],
  );

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

      {/* 网络环境（spec 002 US2：归类调整入口；spec 010 T5 起旧告警条随 443
          Private 语义退役，公用 × 例外的提示由访问白名单区承接） */}
      <section class="card">
        <h2 class="card__title">{t("net.title", lang)}</h2>
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

      {/* 访问通道（spec 008：组网单通道）：组网状态 + 虚拟 IP/成员 + DNS 指引 +
          访问白名单区（spec 010，健康快照与复测回调下沉） */}
      <MeshCard
        lang={lang}
        settings={settings}
        meshStatus={meshStatus}
        lanHealth={lanHealth}
        onLanRefresh={refreshLan}
        onToast={onToast}
      />

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
                {/* 局域网行三态措辞（spec 010 AC5/AC8）：off/pending → 已收口、
                    on → 临时放行剩余时长、expired → 回落未完成（get_urls 语义不动）；
                    前置绿/红二值豆（验收期第 4 项，与域名行心跳同色语义：
                    绿=此路通 / 红=此路不通，默认收口态也红） */}
                {k === "lan" ? (
                  <>
                    <span class={`hb-dot ${lanDotGreen ? "hb-dot--ok" : "hb-dot--fail"}`} />
                    <LanAddrChip health={lanHealth} lang={lang} />
                  </>
                ) : null}
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

/** 地址区「局域网」行三态措辞映射（spec 010 AC5/AC8；纯函数供测试：
 * 例外 on → 临时放行 + 剩余整小时；expired → 已到期回落未完成；
 * off / pending / 尚无探测 → 已收口——pending 为「已请求但规则未生效」，
 * 直访同样不通，按收口如实呈现（MeshCard 另行引导重新开启） */
export function lanAddrState(exception: ExceptionState | null): {
  key: DictKey;
  tone: "closed" | "open" | "expired";
  hours: number | null;
} {
  switch (exception?.state) {
    case "on":
      return {
        key: "languard.addrOn",
        tone: "open",
        hours: Math.ceil(exception.remainingSecs / 3600),
      };
    case "expired":
      return { key: "languard.addrExpired", tone: "expired", hours: null };
    default:
      return { key: "languard.addrOff", tone: "closed", hours: null };
  }
}

/** 三态 → chip 配色（收口灰 / 放行橙警 / 到期红） */
function lanAddrChipClass(tone: "closed" | "open" | "expired"): string {
  switch (tone) {
    case "open":
      return "chip chip--net-public";
    case "expired":
      return "chip chip--failed";
    default:
      return "chip chip--stopped";
  }
}

/** 局域网行状态 chip（消费 LanHealth；get_urls 语义不动，措辞由前端组装） */
function LanAddrChip(props: { health: LanHealth | null; lang: Lang }) {
  const { health, lang } = props;
  const s = lanAddrState(health?.exception ?? null);
  const text = t(s.key, lang).replace("{h}", String(s.hours ?? 0));
  return <span class={lanAddrChipClass(s.tone)}>{text}</span>;
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
function fmtElapsed(ms: number): string {
  const secs = Math.max(0, Math.floor(ms / 1000));
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m${String(secs % 60).padStart(2, "0")}s`;
  const hours = Math.floor(mins / 60);
  return `${hours}h${String(mins % 60).padStart(2, "0")}m`;
}
