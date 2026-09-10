/**
 * 访问通道卡（spec 004 T9/T10：AC5/6/7/11/12/13 展示层）。
 * - 当前通道 chip + 切换按钮：确认流程 30s 未确认自动还原（沿 002 确认模式）
 * - 隧道状态行：tunnel://status 事件驱动（not-configured/disabled/inactive/
 *   starting/online/offline 六态 + 失败摘要）
 * - DNS 指引：穿透通道或「切回直连未恢复」时 30s 周期权威检测，对齐即消失；
 *   CNAME 目标不符时明示实际值（AC12/13）
 * 通道与配置数据源：settings（App 持有）；切换成功后经 onSettingsChange 回写。
 */

import { useEffect, useState } from "preact/hooks";
import { api } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  AccessChannel,
  DnsAlignment,
  Settings,
  TunnelStatus,
} from "../types";

/** 体检单项结论 */
interface CheckItem {
  label: string;
  /** true=正常 false=异常 null=不适用（直连模式下的隧道项） */
  ok: boolean | null;
  detail?: string;
}

export interface TunnelCardProps {
  lang: Lang;
  settings: Settings | null;
  tunnelStatus: TunnelStatus | null;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
  /** 切换/开关成功后的设置回写（App 层 setSettings） */
  onSettingsChange: (s: Settings) => void;
}

/** DNS 检测周期（指引可见期间；对齐后循环自然停止） */
const DNS_CHECK_INTERVAL = 30_000;

export function TunnelCard(props: TunnelCardProps) {
  const { lang, settings, tunnelStatus, onToast, onSettingsChange } = props;
  const channel = settings?.accessChannel ?? "direct";
  const tunnelCfg = settings?.tunnel ?? null;

  // 切换两步确认（30s 未确认自动还原，沿 002 网络归类确认模式）
  const [confirm, setConfirm] = useState<AccessChannel | null>(null);
  const [switching, setSwitching] = useState(false);
  useEffect(() => {
    if (confirm === null) return;
    const id = setTimeout(() => setConfirm(null), 30_000);
    return () => clearTimeout(id);
  }, [confirm]);

  // DNS 对齐检测：穿透通道常态轮询；直连通道仅在「有未对齐结论待恢复」时轮询
  const [dns, setDns] = useState<DnsAlignment | null>(null);
  const [dnsChecking, setDnsChecking] = useState(false);
  // 通道体检（聚合 DNS/归类/隧道/本机组件/域名全链路，复用既有检查通道）
  const [checkup, setCheckup] = useState<CheckItem[] | null>(null);
  const [checking, setChecking] = useState(false);
  const checkDns = () => {
    if (dnsChecking) return;
    setDnsChecking(true);
    api
      .checkDnsAlignment()
      .then(setDns)
      .catch(() => setDns({ kind: "queryFailed" }))
      .finally(() => setDnsChecking(false));
  };
  useEffect(() => {
    // channel != tunnel 时若从未检测（常态直连）则不启动循环（降噪，spec §4）
    const pendingRestore =
      channel === "direct" && dns !== null && dns.kind !== "alignedDirect";
    if (channel !== "tunnel" && !pendingRestore) return;
    checkDns();
    const id = setInterval(checkDns, DNS_CHECK_INTERVAL);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [channel, dns?.kind]);

  const doSwitch = async (target: AccessChannel) => {
    setConfirm(null);
    setSwitching(true);
    try {
      const saved = await api.switchChannel(target);
      onSettingsChange(saved);
      setDns(null);
      onToast(t("tunnel.switched", lang), "success");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setSwitching(false);
    }
  };

  const toggleEnabled = async (enabled: boolean) => {
    try {
      onSettingsChange(await api.setTunnelEnabled(enabled));
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    }
  };

  const configured = tunnelCfg !== null;
  const state = tunnelStatus?.state ?? (configured ? null : "notConfigured");

  /** 通道体检（需求方 2026-09-10：复用 DNS/归类/隧道/组件/可达性检查知识） */
  const runCheckup = async () => {
    setChecking(true);
    try {
      const [netR, dnsR, tunR, compR, healthR] = await Promise.allSettled([
        api.getNetStatus(),
        api.checkDnsAlignment(),
        api.getTunnelStatus(),
        api.getStatus(),
        api.checkDomainHealthNow(),
      ]);
      const items: CheckItem[] = [];
      const okText = t("tunnel.check.ok", lang);
      const failText = t("tunnel.check.fail", lang);

      // ① DNS 解析对齐（权威口径，与上方指引同源）
      if (dnsR.status === "fulfilled") {
        const d = dnsR.value;
        const aligned =
          channel === "tunnel" ? d.kind === "alignedTunnel" : d.kind === "alignedDirect";
        items.push({
          label: t("tunnel.check.dns", lang),
          ok: aligned,
          detail: aligned ? okText : t("tunnel.checkupDnsHint", lang),
        });
      } else {
        items.push({ label: t("tunnel.check.dns", lang), ok: false, detail: failText });
      }

      // ② 网络归类（spec 002 知识：Public 下 443 规则不生效）
      const net = netR.status === "fulfilled" ? netR.value : null;
      if (net) {
        const publicNet = net.networks.filter((n) => n.category === "public");
        const alert = net.rulePrivateOnly && publicNet.length > 0;
        items.push({
          label: t("tunnel.check.netCategory", lang),
          ok: !alert,
          detail: alert
            ? t("tunnel.check.netPublicWarn", lang).replace(
                "{names}",
                publicNet.map((n) => n.name).join("、"),
              )
            : okText,
        });
      }

      // ③ 隧道客户端（穿透模式判状态；直连模式不适用）
      if (tunR.status === "fulfilled") {
        const s = tunR.value.state;
        if (channel === "tunnel") {
          const ok = s === "online" || s === "starting";
          items.push({
            label: t("tunnel.check.tunnel", lang),
            ok,
            detail: t(tunnelStateKey(s), lang),
          });
        } else {
          items.push({
            label: t("tunnel.check.tunnel", lang),
            ok: null,
            detail: t("tunnel.check.tunnelOff", lang),
          });
        }
      }

      // ④⑤ 本机组件（caddy 443 / 上游 3001，spec 001 probe 快照）
      if (compR.status === "fulfilled") {
        for (const id of ["caddy", "cloudcli"] as const) {
          const c = compR.value.find((s) => s.id === id);
          const running = c?.state === "running";
          items.push({
            label: t(id === "caddy" ? "tunnel.check.caddy" : "tunnel.check.upstream", lang),
            ok: running,
            detail: running ? okText : t(`common.${c?.state ?? "stopped"}` as DictKey, lang),
          });
        }
      }

      // ⑥ 域名全链路（本机视角，spec 005 口径如实标注）
      if (healthR.status === "fulfilled") {
        const h = healthR.value;
        items.push({
          label: t("tunnel.check.domain", lang),
          ok: h.kind === "ok",
          detail:
            h.kind === "ok"
              ? `${h.latencyMs}ms`
              : t(`heartbeat.kind.${h.kind}` as DictKey, lang).replace(
                  "{code}",
                  String(h.code ?? ""),
                ),
        });
      } else {
        items.push({ label: t("tunnel.check.domain", lang), ok: false, detail: failText });
      }

      setCheckup(items);
    } finally {
      setChecking(false);
    }
  };

  return (
    <section class="card">
      <h2 class="card__title">{t("tunnel.title", lang)}</h2>

      {/* 当前通道 + 切换入口 */}
      <div class="net__row">
        <span class="net__name">{t("tunnel.channelLabel", lang)}</span>
        <span class={`chip ${channel === "tunnel" ? "chip--net-private" : "chip--net-domain"}`}>
          {t(channel === "tunnel" ? "tunnel.channelTunnel" : "tunnel.channelDirect", lang)}
        </span>
        <span class="net__spacer" />
        {confirm === null ? (
          <button
            class="btn btn--sm"
            disabled={switching}
            onClick={() => {
              if (channel === "direct" && !configured) {
                onToast(t("tunnel.notConfigured", lang), "error");
                return;
              }
              setConfirm(channel === "direct" ? "tunnel" : "direct");
            }}
          >
            {switching
              ? t("tunnel.switching", lang)
              : channel === "direct"
                ? t("tunnel.switchToTunnel", lang)
                : t("tunnel.switchToDirect", lang)}
          </button>
        ) : null}
      </div>

      {/* 切换确认（风险/步骤先行，30s 自动还原） */}
      {confirm !== null ? (
        <div class="net__confirm">
          <p class="net__risk">
            <strong>
              {t(
                confirm === "tunnel" ? "tunnel.confirmTitleTunnel" : "tunnel.confirmTitleDirect",
                lang,
              )}
            </strong>
          </p>
          <p class="net__risk">
            {t(
              confirm === "tunnel" ? "tunnel.confirmStepsTunnel" : "tunnel.confirmStepsDirect",
              lang,
            )}
          </p>
          <button class="btn btn--sm btn--primary" disabled={switching} onClick={() => void doSwitch(confirm)}>
            {switching ? t("tunnel.switching", lang) : t("tunnel.confirm", lang)}
          </button>
          <button class="btn btn--sm" disabled={switching} onClick={() => setConfirm(null)}>
            {t("tunnel.cancel", lang)}
          </button>
        </div>
      ) : null}

      {/* 穿透通道下的启用开关（AC11 停用语义；未配置不展示） */}
      {channel === "tunnel" && configured ? (
        <div class="net__row">
          <span class="net__name">{t("tunnel.enabledLabel", lang)}</span>
          <span class="net__spacer" />
          <button class="btn btn--sm" disabled={switching} onClick={() => void toggleEnabled(!settings?.tunnelEnabled)}>
            {settings?.tunnelEnabled ? t("common.stop", lang) : t("common.start", lang)}
          </button>
        </div>
      ) : null}

      {/* 隧道状态行 */}
      <div class="net__row">
        <span class="net__name">{t("tunnel.statusLabel", lang)}</span>
        {state ? (
          <>
            <span class={`chip ${tunnelChipClass(state)}`}>
              {t(tunnelStateKey(state), lang)}
            </span>
            {tunnelStatus?.detail ? (
              <span class="settings__desc">{tunnelStatus.detail}</span>
            ) : null}
          </>
        ) : (
          <span class="muted">—</span>
        )}
        <span class="net__spacer" />
        {channel === "tunnel" && configured && settings?.tunnelEnabled ? (
          <button
            class="btn btn--sm"
            disabled={switching}
            onClick={() =>
              api
                .restartTunnel()
                .then(() => onToast(t("tunnel.restarted", lang), "success"))
                .catch((e) => onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error"))
            }
          >
            {t("tunnel.restart", lang)}
          </button>
        ) : null}
      </div>

      {/* DNS 指引（穿透待切换 / 直连待恢复 / 目标不符 / 已对齐；AC12/13） */}
      {dns ? <DnsNotice dns={dns} channel={channel} target={tunnelCfg?.nodeDomain ?? ""} lang={lang} /> : null}
      {channel === "tunnel" || (dns !== null && dns.kind !== "alignedDirect") ? (
        <div class="settings__actions">
          <button class="btn btn--sm" disabled={dnsChecking} onClick={checkDns}>
            {dnsChecking ? t("tunnel.dnsChecking", lang) : t("tunnel.dnsRecheck", lang)}
          </button>
        </div>
      ) : null}

      {/* 通道体检（复用 DNS/归类/隧道/组件/全链路检查知识） */}
      <div class="settings__actions">
        <button class="btn btn--sm" disabled={checking} onClick={() => void runCheckup()}>
          {checking ? t("tunnel.checkupRunning", lang) : t("tunnel.checkupRun", lang)}
        </button>
      </div>
      {checkup ? (
        <div class="checkup__list">
          {checkup.map((item) => (
            <div class="net__row" key={item.label}>
              <span class="net__name">{item.label}</span>
              <span
                class={`chip ${item.ok === null ? "chip--stopped" : item.ok ? "chip--running" : "chip--failed"}`}
              >
                {item.ok === null ? "—" : item.ok ? t("tunnel.check.ok", lang) : t("tunnel.check.fail", lang)}
              </span>
              {item.detail ? <span class="settings__desc">{item.detail}</span> : null}
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}

/** DNS 结论 → 提示条（kind 色 + 文案；AC12 检测一致后指引自然退场）。
 * 「对齐」判定与当前通道配对才算完成：穿透通道下残留 A 记录 = 待切换
 * （显示切换指引而非"已恢复直连"），反之亦然——避免通道盲区误导。 */
function DnsNotice(props: {
  dns: DnsAlignment;
  channel: AccessChannel;
  target: string;
  lang: Lang;
}) {
  const { dns, channel, target, lang } = props;
  switch (dns.kind) {
    case "alignedTunnel":
      return channel === "tunnel" ? (
        <p class="notice notice--ok">{t("tunnel.dnsOkTunnel", lang)}</p>
      ) : (
        <p class="notice notice--warn">
          {t("tunnel.dnsGuideDirect", lang).replace("{target}", target)}
        </p>
      );
    case "alignedDirect":
      return channel === "direct" ? (
        <p class="notice notice--ok">{t("tunnel.dnsOkDirect", lang)}</p>
      ) : (
        <p class="notice notice--warn">
          {t("tunnel.dnsGuideTunnel", lang).replace("{target}", target)}
        </p>
      );
    case "mismatchedCname":
      return (
        <p class="notice notice--warn">
          {t("tunnel.dnsMismatch", lang)
            .replace("{actual}", dns.actual)
            .replace("{target}", target)}
        </p>
      );
    case "noRecord":
      return (
        <p class="notice notice--warn">
          {t(
            channel === "tunnel" ? "tunnel.dnsGuideTunnel" : "tunnel.dnsGuideDirect",
            lang,
          ).replace("{target}", target)}
        </p>
      );
    case "queryFailed":
      return <p class="notice notice--warn">{t("tunnel.dnsFailed", lang)}</p>;
  }
}

/** 隧道六态 → chip 配色（复用组件五态色板） */
function tunnelChipClass(state: TunnelStatus["state"]): string {
  switch (state) {
    case "online":
      return "chip--running";
    case "starting":
      return "chip--starting";
    case "offline":
      return "chip--failed";
    default:
      return "chip--stopped";
  }
}

/** 隧道六态 → 词典键 */
function tunnelStateKey(state: TunnelStatus["state"]): DictKey {
  return `tunnel.state.${state}` as DictKey;
}
