/**
 * 装机向导视图（spec 006 §4.1 + 008 通道单化：组网为唯一通道分支）。
 * - 左侧竖向阶段清单（四态 chip）+ 右侧当前阶段面板；已完成阶段可点击回看（AC2）
 * - 外部办理统一模式：文字步骤 + [打开××] + [我已完成，开始校验]（校验 = wizard_detect）
 * - 凭证零 APP 化：密钥输入拉起控制台脚本（run_tool），UI 只呈现结果码（宪法 §3）
 * - 通道阶段 = 组网装服务/成员入网/应用配置/同步 DNS 的装机序列（首装路径）；
 *   装机后的日常维护入口已下沉主看板 MeshCard（spec 008 T15）。
 *   直连/穿透分支与向导 branch 字段已随通道退役删除——旧状态文件同名键
 *   被 Rust serde 忽略（spec 008）
 * - detail 稳定码 → i18n 呈现；失败只伤单阶段 + 重试（AC12）
 * - 收尾页「访问白名单/旧规则」检查项（spec 010 T6）：消费 lan_guard_status，
 *   旧规则残留给「一键收口」、白名单失配给「修复白名单」（与主看板同命令）
 */

import { useEffect, useState } from "preact/hooks";
import { api, onWizardChanged } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  LanHealth,
  Settings,
  StageState,
  StageStatus,
  WizardStageId,
  WizardState,
} from "../types";
import { CopyButton } from "./CopyButton";

export interface WizardViewProps {
  lang: Lang;
  settings: Settings | null;
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
  const { lang, settings, focusStage, onToast, onFinished } = props;
  const [state, setState] = useState<WizardState | null>(null);
  /** 展开面板的阶段（null = 跟随首个未完成阶段） */
  const [open, setOpen] = useState<WizardStageId | null>(null);
  /** 在途动作键（防重复点击） */
  const [busy, setBusy] = useState<string | null>(null);
  const [domainDraft, setDomainDraft] = useState("");

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

  // 访问白名单/旧规则检查（spec 010 T6 收尾页）：进入向导即探测一次；
  // 动作后即时/延迟复测（UAC 窗内规则数秒后落位，60s 监视器兜尾）
  const [lanHealth, setLanHealth] = useState<LanHealth | null>(null);
  useEffect(() => {
    let disposed = false;
    api
      .lanGuardStatus()
      .then((h) => {
        if (!disposed) setLanHealth(h);
      })
      .catch(() => {});
    return () => {
      disposed = true;
    };
  }, []);
  const refreshLan = () => {
    api.lanGuardStatus().then(setLanHealth).catch(() => {});
  };
  /** 白名单动作：沿 dispatch 提示成败 + 复测（3.5s/12s 追加，监视器兜尾） */
  const runLanGuard = async (key: string, action: () => Promise<unknown>) => {
    await dispatch(key, action, false);
    refreshLan();
    setTimeout(refreshLan, 3500);
    setTimeout(refreshLan, 12000);
  };

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

  /** 校验（统一收口动作）：重探测后给出明确结论——首个未完成阶段指名 + 原因，
   * 全过报通过。2026-09-11 需求方反馈：此前点校验只有阶段标记静默变化、
   * 无成败反馈，「等待成员加入」这类非错误态尤其无从知晓原因 */
  const detect = async () => {
    setBusy("detect");
    try {
      const s = await api.wizardDetect();
      const bad = s.stages.find((x) => x.state !== "done" && x.state !== "skipped");
      if (bad) {
        const why = bad.detail
          ? t(`wizard.code.${bad.detail}` as DictKey, lang)
          : t("wizard.state.pending", lang);
        onToast(
          `${t("wizard.checkFailed", lang)}：${t(`wizard.stage.${bad.id}`, lang)}——${why}`,
          "error",
        );
      } else {
        onToast(t("wizard.checkPassed", lang), "success");
      }
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setBusy(null);
    }
  };

  /** 组网 DNS 同步（AC13）：独立入口——向导分支选择不做编排，A=虚拟 IP 的
   * 记录由此创建；返回操作数透出（CNAME 删了几条 + A upsert） */
  const syncMeshDns = async () => {
    setBusy("meshDns");
    try {
      const n = await api.meshSyncDns();
      onToast(t("wizard.channel.meshDnsDone", lang).replace("{n}", String(n)), "success");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setBusy(null);
    }
  };

  /** 校验按钮（外部办理统一模式的收口动作，AC4/5/9/10） */
  const CheckButton = () => (
    <button class="btn btn--primary btn--sm" disabled={busy !== null} onClick={() => void detect()}>
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
            <button
              class="btn btn--primary"
              disabled={busy !== null}
              onClick={() =>
                dispatch(
                  "meshSecret",
                  () => api.runTool("set_mesh_secret", { update: false, mirror: false, domain: null }),
                  true,
                )
              }
            >
              {t("wizard.channel.meshSecretBtn", lang)}
            </button>
            <p class="muted">{t("wizard.channel.meshSecretHint", lang)}</p>
            <button
              class="btn"
              disabled={busy !== null}
              onClick={() => dispatch("meshInstall", () => api.meshInstallService(), true)}
            >
              {t("wizard.channel.meshInstallBtn", lang)}
            </button>
            <p class="muted">{t("wizard.channel.meshServiceHint", lang)}</p>
            <p class="wizard__desc">
              {t("wizard.channel.meshPeerGuide", lang).replace(
                "{name}",
                settings?.mesh.networkName ?? "",
              )}
            </p>
            <button
              class="btn"
              disabled={busy !== null}
              onClick={() =>
                void api.openExternal("easytier_releases").catch((e) =>
                  onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"),
                )
              }
            >
              {t("wizard.channel.meshDownloadBtn", lang)}
            </button>
            <div class="wizard__row">
              <code class="wizard__url">{settings?.mesh.networkName}</code>
              <CopyButton
                text={settings?.mesh.networkName ?? ""}
                lang={lang}
                onToast={onToast}
              />
              <span class="muted">{settings?.mesh.virtualIp}</span>
            </div>
            <button
              class="btn btn--sm"
              disabled={busy !== null}
              onClick={() => dispatch("meshApply", () => api.meshApplyConfig(), false)}
            >
              {t("wizard.channel.meshApplyBtn", lang)}
            </button>
            <button
              class="btn"
              disabled={busy !== null}
              onClick={() => void syncMeshDns()}
            >
              {busy === "meshDns" ? t("tunnel.dnsChecking", lang) : t("wizard.channel.meshDnsBtn", lang)}
            </button>
            <p class="muted">{t("wizard.channel.meshDnsHint", lang)}</p>
            <Detail s={s} />
            <CheckButton />
          </>
        );
      case "finalize": {
        // 访问白名单检查项（spec 010）：旧规则残留 → 一键收口；白名单失配 → 修复；
        // ok/dormant 视为达成（dormant = 组网未运行的休眠态，非异常）
        const wl = lanHealth?.whitelist ?? null;
        const legacy = lanHealth?.legacyPresent ?? false;
        const wlBroken = wl === "missing" || wl === "staleCidr" || wl === "staleIface";
        const lanDotOk = !legacy && !wlBroken;
        const lanDetail = legacy
          ? t("languard.legacyBanner", lang)
          : wl === null
            ? t("common.loading", lang)
            : wl === "ok"
              ? t("languard.wl.okHint", lang)
              : wl === "dormant"
                ? t("languard.wl.dormantHint", lang)
                : t("languard.wl.fixHint", lang);
        return (
          <>
            <p class="wizard__desc">{t("wizard.finalize.desc", lang)}</p>
            <ul class="wizard__summary">
              <li>
                <span class={`hb-dot ${stageOf("basis").state === "done" ? "hb-dot--ok" : "hb-dot--fail"}`} />
                <span>{t("wizard.summary.lan", lang)}</span>
              </li>
              <li>
                <span class={`hb-dot ${stageOf("channel").state === "done" ? "hb-dot--ok" : "hb-dot--fail"}`} />
                <span>
                  {t("wizard.summary.domain", lang)} <code>{state.domain}</code>
                </span>
              </li>
              <li>
                <span class={`hb-dot ${lanDotOk ? "hb-dot--ok" : "hb-dot--fail"}`} />
                <span>
                  {t("languard.title", lang)}
                  <span class="muted">：{lanDetail}</span>
                </span>
                {legacy ? (
                  <button
                    class="btn btn--sm"
                    disabled={busy !== null}
                    onClick={() => void runLanGuard("migrate", () => api.lanGuardMigrate())}
                  >
                    {t("languard.migrateBtn", lang)}
                  </button>
                ) : wlBroken ? (
                  <button
                    class="btn btn--sm"
                    disabled={busy !== null}
                    onClick={() =>
                      void runLanGuard("wlFix", () => api.lanGuardEnsureWhitelist())
                    }
                  >
                    {t("languard.fixBtn", lang)}
                  </button>
                ) : null}
              </li>
            </ul>
            <p class="muted">{t("wizard.finalize.meshHint", lang)}</p>
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
