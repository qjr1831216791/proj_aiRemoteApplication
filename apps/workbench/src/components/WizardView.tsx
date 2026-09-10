/**
 * 装机向导视图（spec 006 §4.1）。
 * - 左侧竖向阶段清单（四态 chip）+ 右侧当前阶段面板；已完成阶段可点击回看（AC2）
 * - 外部办理统一模式：文字步骤 + [打开××] + [我已完成，开始校验]（校验 = wizard_detect）
 * - 凭证零 APP 化：密钥输入拉起控制台脚本（run_tool），UI 只呈现结果码（宪法 §3）
 * - 分支选择写 Settings.access_channel（frpc/ddns-go 收敛复用 004 守护，AC9）
 * - detail 稳定码 → i18n 呈现；失败只伤单阶段 + 重试（AC12）
 */

import { useEffect, useState } from "preact/hooks";
import { api, onWizardChanged } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  Settings,
  StageState,
  StageStatus,
  TunnelStatus,
  WizardStageId,
  WizardState,
} from "../types";
import { CopyButton } from "./CopyButton";

export interface WizardViewProps {
  lang: Lang;
  settings: Settings | null;
  tunnelStatus: TunnelStatus | null;
  /** AC14：组件卡「去安装」跳转时定位的阶段 */
  focusStage: WizardStageId | null;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
  onSettingsChange: (s: Settings) => void;
  /** 完成后回主看板（AC11） */
  onFinished: () => void;
}

const STAGE_ORDER: WizardStageId[] = ["basis", "tencent", "https", "channel", "finalize"];

const STAGE_LABEL: Record<WizardStageId, DictKey> = {
  basis: "wizard.stage.basis",
  tencent: "wizard.stage.tencent",
  https: "wizard.stage.https",
  channel: "wizard.stage.channel",
  finalize: "wizard.stage.finalize",
};

const STATE_CHIP: Record<StageState, string> = {
  pending: "chip chip--wiz-pending",
  done: "chip chip--wiz-done",
  failed: "chip chip--wiz-failed",
  skipped: "chip chip--wiz-skipped",
};

const STATE_LABEL: Record<StageState, DictKey> = {
  pending: "wizard.state.pending",
  done: "wizard.state.done",
  failed: "wizard.state.failed",
  skipped: "wizard.state.skipped",
};

export function WizardView(props: WizardViewProps) {
  const { lang, settings, tunnelStatus, focusStage, onToast, onSettingsChange, onFinished } = props;
  const [state, setState] = useState<WizardState | null>(null);
  /** 展开面板的阶段（null = 跟随首个未完成阶段） */
  const [open, setOpen] = useState<WizardStageId | null>(null);
  /** 在途动作键（防重复点击） */
  const [busy, setBusy] = useState<string | null>(null);
  const [domainDraft, setDomainDraft] = useState("");
  const [tunnelId, setTunnelId] = useState(settings?.tunnel?.tunnelId ?? "");
  const [nodeDomain, setNodeDomain] = useState(settings?.tunnel?.nodeDomain ?? "");

  useEffect(() => {
    let disposed = false;
    void (async () => {
      const s = await api.wizardGetState().catch(() => null);
      if (!disposed && s) {
        setState(s);
        setDomainDraft(s.domain);
      }
    })();
    const unsub = onWizardChanged((s) => {
      setState(s);
      setDomainDraft(s.domain);
    });
    return () => {
      disposed = true;
      unsub.then((u) => u());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // AC14：外部跳转定位
  useEffect(() => {
    if (focusStage) setOpen(focusStage);
  }, [focusStage]);

  if (!state) {
    return (
      <section class="card">
        <p class="muted">{t("common.loading", lang)}</p>
      </section>
    );
  }

  const stageOf = (id: WizardStageId): StageStatus =>
    state.stages.find((s) => s.id === id) ?? { id, state: "pending", detail: null };
  const firstOpen = STAGE_ORDER.find((id) => {
    const st = stageOf(id).state;
    return st === "pending" || st === "failed";
  });
  const active = open ?? firstOpen ?? "finalize";

  const dispatch = async (key: string, action: () => Promise<unknown>, doneHint: boolean) => {
    setBusy(key);
    try {
      await action();
      onToast(t("tools.dispatched", lang), "success");
      if (doneHint) onToast(t("wizard.checkHint", lang), "info");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setBusy(null);
    }
  };

  const detect = () =>
    dispatch(
      "detect",
      () => api.wizardDetect(),
      false,
    );

  /** 校验按钮（外部办理统一模式的收口动作，AC4/5/9/10） */
  const CheckButton = () => (
    <button class="btn btn--primary btn--sm" disabled={busy !== null} onClick={detect}>
      {busy === "detect" ? t("wizard.checking", lang) : t("wizard.check", lang)}
    </button>
  );

  const Detail = ({ s }: { s: StageStatus }) =>
    s.detail ? (
      <p class={`wizard__detail${s.state === "failed" ? " wizard__detail--bad" : ""}`}>
        {t(`wizard.code.${s.detail}` as DictKey, lang)}
      </p>
    ) : null;

  const stagePanel = (id: WizardStageId) => {
    const s = stageOf(id);
    switch (id) {
      case "basis":
        return (
          <>
            <p class="wizard__desc">{t("wizard.basis.desc", lang)}</p>
            <button
              class="btn btn--primary"
              disabled={busy !== null}
              onClick={() =>
                dispatch("basis", () => api.runTool("install_server", { update: false, mirror: false, domain: null }), true)
              }
            >
              {t("wizard.basis.run", lang)}
            </button>
            <Detail s={s} />
            <CheckButton />
          </>
        );
      case "tencent":
        return (
          <>
            <p class="wizard__desc">{t("wizard.tencent.desc", lang)}</p>
            <ol class="wizard__steps">
              <li>
                {t("wizard.tencent.step1", lang)}{" "}
                <code class="wizard__url">https://console.cloud.tencent.com/cam/capi</code>
                <CopyButton text="https://console.cloud.tencent.com/cam/capi" lang={lang} onToast={onToast} />
              </li>
              <li>{t("wizard.tencent.step2", lang)}</li>
              <li>{t("wizard.tencent.step3", lang)}</li>
            </ol>
            <div class="wizard__row">
              <label class="wizard__label">{t("wizard.domainLabel", lang)}</label>
              <input
                class="wizard__input"
                value={domainDraft}
                onInput={(e) => setDomainDraft((e.target as HTMLInputElement).value)}
              />
              <button
                class="btn btn--sm"
                disabled={busy !== null || domainDraft === state.domain}
                onClick={() => dispatch("domain", () => api.wizardSetDomain(domainDraft), false)}
              >
                {t("wizard.domainSave", lang)}
              </button>
            </div>
            <button
              class="btn"
              disabled={busy !== null}
              onClick={() => dispatch("tencentKey", () => api.runTool("set_tencent_key", { update: false, mirror: false, domain: null }), true)}
            >
              {t("wizard.tencent.keyBtn", lang)}
            </button>
            <Detail s={s} />
            <CheckButton />
          </>
        );
      case "https":
        return (
          <>
            <p class="wizard__desc">{t("wizard.https.desc", lang)}</p>
            <button
              class="btn btn--primary"
              disabled={busy !== null}
              onClick={() =>
                dispatch(
                  "https",
                  () => api.runTool("install_https", { update: false, mirror: false, domain: state.domain }),
                  true,
                )
              }
            >
              {t("wizard.https.run", lang)}
            </button>
            <Detail s={s} />
            <CheckButton />
          </>
        );
      case "channel":
        return (
          <>
            <p class="wizard__desc">{t("wizard.channel.desc", lang)}</p>
            <div class="wizard__cards">
              <button
                class={`card wizard__choice${state.branch === "direct" ? " wizard__choice--on" : ""}`}
                onClick={() => dispatch("branch", () => api.wizardSetBranch("direct"), false)}
              >
                <strong>{t("wizard.channel.direct", lang)}</strong>
                <span>{t("wizard.channel.directDesc", lang)}</span>
              </button>
              <button
                class={`card wizard__choice${state.branch === "tunnel" ? " wizard__choice--on" : ""}`}
                onClick={() => dispatch("branch", () => api.wizardSetBranch("tunnel"), false)}
              >
                <strong>{t("wizard.channel.tunnel", lang)}</strong>
                <span>{t("wizard.channel.tunnelDesc", lang)}</span>
              </button>
            </div>
            {state.branch === "direct" ? (
              <>
                <button
                  class="btn btn--primary"
                  disabled={busy !== null}
                  onClick={() =>
                    dispatch(
                      "ddns",
                      () => api.runTool("config_ddnsgo", { update: false, mirror: false, domain: state.domain }),
                      true,
                    )
                  }
                >
                  {t("wizard.channel.ddnsBtn", lang)}
                </button>
                {s.detail?.startsWith("warn_") || s.detail === "record_mismatch" ? (
                  <p class="notice notice--warn">{t(`wizard.code.${s.detail}` as DictKey, lang)}</p>
                ) : null}
              </>
            ) : null}
            {state.branch === "tunnel" ? (
              <>
                <button
                  class="btn"
                  disabled={busy !== null}
                  onClick={() => dispatch("frpKey", () => api.runTool("set_frp_key", { update: false, mirror: false, domain: null }), true)}
                >
                  {t("wizard.channel.frpKeyBtn", lang)}
                </button>
                <div class="wizard__row">
                  <label class="wizard__label">{t("wizard.channel.tunnelId", lang)}</label>
                  <input class="wizard__input" value={tunnelId} onInput={(e) => setTunnelId((e.target as HTMLInputElement).value)} />
                </div>
                <div class="wizard__row">
                  <label class="wizard__label">{t("wizard.channel.nodeDomain", lang)}</label>
                  <input class="wizard__input" value={nodeDomain} onInput={(e) => setNodeDomain((e.target as HTMLInputElement).value)} />
                </div>
                <button
                  class="btn btn--sm"
                  disabled={busy !== null || !/^\d+$/.test(tunnelId) || !nodeDomain}
                  onClick={() =>
                    dispatch(
                      "tunnelCfg",
                      () =>
                        api
                          .saveSettings({ tunnel: { tunnelId, nodeDomain } })
                          .then(onSettingsChange),
                      false,
                    )
                  }
                >
                  {t("wizard.channel.saveCfg", lang)}
                </button>
                <p class="wizard__tunnelState">
                  {t("wizard.channel.tunnelState", lang)}: {tunnelStatusLabel(tunnelStatus, lang)}
                </p>
                <p class="muted">{t("wizard.channel.dnsNote", lang)}</p>
              </>
            ) : null}
            <Detail s={s} />
            <CheckButton />
          </>
        );
      case "finalize":
        return (
          <>
            <p class="wizard__desc">{t("wizard.finalize.desc", lang)}</p>
            <ul class="wizard__summary">
              <li>
                {t("wizard.summary.lan", lang)}
                <span class={`hb-dot ${stageOf("basis").state === "done" ? "hb-dot--ok" : "hb-dot--fail"}`} />
              </li>
              <li>
                {t("wizard.summary.domain", lang)} <code>{state.domain}</code>
                <span class={`hb-dot ${stageOf("channel").state === "done" ? "hb-dot--ok" : "hb-dot--fail"}`} />
              </li>
            </ul>
            <button
              class="btn"
              disabled={busy !== null}
              onClick={() =>
                dispatch(
                  "autostart",
                  () => api.setAutostartServices(true).then(() => api.setAutostartApp(true)),
                  false,
                )
              }
            >
              {t("wizard.finalize.autostart", lang)}
            </button>
            <button
              class="btn btn--primary"
              disabled={busy !== null}
              onClick={() =>
                dispatch("done", () => api.wizardComplete(), false).then(onFinished)
              }
            >
              {t("wizard.finalize.done", lang)}
            </button>
          </>
        );
    }
  };

  return (
    <div class="wizard">
      <p class="wizard__hint">{t("wizard.hint", lang)}</p>
      <div class="wizard__layout">
        <nav class="wizard__rail">
          {STAGE_ORDER.map((id, i) => {
            const s = stageOf(id);
            return (
              <button
                key={id}
                class={`wizard__stage${active === id ? " wizard__stage--on" : ""}`}
                onClick={() => setOpen(id)}
              >
                <span class="wizard__num">{i + 1}</span>
                <span>{t(STAGE_LABEL[id], lang)}</span>
                <span class={STATE_CHIP[s.state]}>{t(STATE_LABEL[s.state], lang)}</span>
              </button>
            );
          })}
        </nav>
        <section class="card wizard__panel">
          <h2 class="card__title">
            {t(STAGE_LABEL[active], lang)}
          </h2>
          {stagePanel(active)}
        </section>
      </div>
    </div>
  );
}

function tunnelStatusLabel(s: TunnelStatus | null, lang: Lang): string {
  if (!s) return t("wizard.state.pending", lang);
  return t(`tunnel.state.${s.state}` as DictKey, lang);
}
