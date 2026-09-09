/**
 * 主界面（T13：AC1/4/6 展示层）。
 * - 总开关：启动/停止双按钮，进行中禁用防重复；部分失败不显示笼统"启动成功"，
 *   实况由各组件状态卡自述（AC6）
 * - 三组件状态卡：五态色、端口、当前态耗时（since）、失败/port-held 原因、
 *   组件级重试（AC6）
 * - 地址区：本机/局域网/域名三行，一键复制 + 打开（AC19 地址区）
 * 状态数据流：status://changed 事件（App 订阅）+ 启动时 get_status 兜底。
 */

import { useEffect, useState } from "preact/hooks";
import { api } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type { AccessUrls, ComponentId, ComponentState, ComponentStatus, ScriptsAvailability } from "../types";
import { CopyButton } from "./CopyButton";
import { ToolsSection } from "./ToolsSection";

export interface MainViewProps {
  lang: Lang;
  statuses: ComponentStatus[];
  urls: AccessUrls | null;
  scripts: ScriptsAvailability | null;
  /** 一键停止在途（防重复点击） */
  stopping: boolean;
  onStartAll: () => void;
  onStopAll: () => void;
  onRetry: (id: ComponentId) => void;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
}

export function MainView(props: MainViewProps) {
  const { lang, statuses, urls, scripts, stopping, onStartAll, onStopAll, onRetry, onToast } = props;
  // 当前态耗时（since → now）每秒刷新
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  const starting = statuses.some((s) => s.state === "starting");
  const anyRunning = statuses.some((s) => s.state === "running");
  const busy = starting || stopping;

  return (
    <>
      {/* 总开关（spec §4.6 信息分区之首） */}
      <section class="card master">
        <div class="master__actions">
          <button class="btn btn--primary" disabled={busy} onClick={onStartAll}>
            {t("main.startAll", lang)}
          </button>
          <button class="btn btn--danger" disabled={busy || !anyRunning} onClick={onStopAll}>
            {t("main.stopAll", lang)}
          </button>
        </div>
        <p class="master__hint">
          {busy ? t("main.busy", lang) : t("main.stopHint", lang)}
        </p>
      </section>

      {/* 三组件状态卡 */}
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
            </p>
            {s.detail ? <p class="status__detail">{s.detail}</p> : null}
            {s.state === "failed" || s.state === "port-held" ? (
              <button class="btn btn--sm" disabled={busy} onClick={() => onRetry(s.id)}>
                {t("common.retry", lang)}
              </button>
            ) : null}
          </article>
        ))}
      </section>

      {/* 地址区 */}
      <section class="card">
        <h2 class="card__title">{t("addr.title", lang)}</h2>
        {urls
          ? (["local", "lan", "domain"] as const).map((k) => (
              <div class="addr__row" key={k}>
                <span class="addr__label">{t(`addr.${k}`, lang)}</span>
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

/** 耗时格式化：48s / 3m24s / 1h05m（语言无关） */
export function fmtElapsed(ms: number): string {
  const secs = Math.max(0, Math.floor(ms / 1000));
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m${String(secs % 60).padStart(2, "0")}s`;
  const hours = Math.floor(mins / 60);
  return `${hours}h${String(mins % 60).padStart(2, "0")}m`;
}
