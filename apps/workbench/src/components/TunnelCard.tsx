/**
 * 访问通道卡（spec 004 T9/T10 + 007 T11：AC5/6/7/11/12/13 + AC1/4/5/6 展示层）。
 * - 三通道切换（direct/tunnel/mesh，mesh 新装机默认推荐）：两步确认 30s 未确认
 *   自动还原（沿 002 确认模式）；停用通道可切换回（确认框附安全警示——公网
 *   暴露面回归，AC5/AC6 重新启用语义）
 * - 隧道状态行：tunnel://status 事件驱动（六态 + 失败摘要）
 * - 组网状态区（现役 mesh 时）：mesh://status 四态 + detail 稳定码 + 成员列表
 *   （peer list 首项恒为本机，isLocal 标注；在线判定在 Rust 侧排除本机项）
 * - DNS 指引：按通道常态轮询权威检测，对齐即消失；mesh 态判 A=虚拟 IP
 *   （007 体检重定义：alignedMesh/mismatchedA），CNAME 残留按旁路暴露面提示
 * 通道与配置数据源：settings（App 持有）；切换成功后经 onSettingsChange 回写。
 */

import { useEffect, useState } from "preact/hooks";
import { api } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  AccessChannel,
  DnsAlignment,
  MeshStateKind,
  MeshStatus,
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
  meshStatus: MeshStatus | null;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
  /** 切换/开关成功后的设置回写（App 层 setSettings） */
  onSettingsChange: (s: Settings) => void;
}

/** DNS 检测周期（指引可见期间；对齐后循环自然停止） */
const DNS_CHECK_INTERVAL = 30_000;

export function TunnelCard(props: TunnelCardProps) {
  const { lang, settings, tunnelStatus, meshStatus, onToast, onSettingsChange } = props;
  const channel = settings?.accessChannel ?? "direct";
  const tunnelCfg = settings?.tunnel ?? null;
  const meshCfg = settings?.mesh ?? null;

  // 切换两步确认（30s 未确认自动还原，沿 002 网络归类确认模式）
  const [confirm, setConfirm] = useState<AccessChannel | null>(null);
  const [switching, setSwitching] = useState(false);
  useEffect(() => {
    if (confirm === null) return;
    const id = setTimeout(() => setConfirm(null), 30_000);
    return () => clearTimeout(id);
  }, [confirm]);

  // DNS 对齐检测：穿透/组网通道常态轮询；直连通道仅在「有未对齐结论待恢复」时轮询
  const [dns, setDns] = useState<DnsAlignment | null>(null);
  const [dnsChecking, setDnsChecking] = useState(false);
  // 通道体检（聚合 DNS/归类/通道客户端/本机组件/域名全链路，复用既有检查通道）
  const [checkup, setCheckup] = useState<CheckItem[] | null>(null);
  const [checking, setChecking] = useState(false);
  // 组网配置应用在途（UAC 派发）
  const [applying, setApplying] = useState(false);
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
    // 直连常态不启动循环（降噪，spec §4）；穿透/组网等 A/CNAME 生效需常态盯
    const pendingRestore =
      channel === "direct" && dns !== null && dns.kind !== "alignedDirect";
    if (channel === "direct" && !pendingRestore) return;
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
      // Rust 侧前置校验拒绝文案自带可读指引（穿透未配置/组网未就绪等）
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setSwitching(false);
    }
  };

  /** 组网配置应用（渲染→校验→UAC 重启/安装；与切换通道共享 Rust 内核） */
  const applyMesh = async () => {
    setApplying(true);
    try {
      await api.meshApplyConfig();
      onToast(t("mesh.applied", lang), "info");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setApplying(false);
    }
  };

  const configured = tunnelCfg !== null;
  const state = tunnelStatus?.state ?? (configured ? null : "notConfigured");
  const mstate: MeshStateKind | null = meshStatus?.state ?? null;

  /** 切换目标按钮次序（mesh 推荐位在前，007 新装机默认） */
  const others: AccessChannel[] =
    channel === "mesh"
      ? ["direct", "tunnel"]
      : ["mesh", channel === "direct" ? "tunnel" : "direct"];
  /** 目标通道停用标记（切换=重新启用，确认框附安全警示） */
  const disabledOf = (ch: AccessChannel): boolean =>
    (ch === "tunnel" && !!settings?.tunnelDisabled) ||
    (ch === "direct" && !!settings?.directDisabled);

  /** 通道体检（需求方 2026-09-10：复用 DNS/归类/通道/组件/可达性检查知识；
   * 007 起③项按通道取隧道客户端或组网服务实况） */
  const runCheckup = async () => {
    setChecking(true);
    try {
      const [netR, dnsR, tunR, meshR, compR, healthR] = await Promise.allSettled([
        api.getNetStatus(),
        api.checkDnsAlignment(),
        api.getTunnelStatus(),
        api.getMeshStatus(),
        api.getStatus(),
        api.checkDomainHealthNow(),
      ]);
      const items: CheckItem[] = [];
      const okText = t("tunnel.check.ok", lang);
      const failText = t("tunnel.check.fail", lang);

      // ① DNS 解析对齐（权威口径，与上方指引同源；mesh 判 A=虚拟 IP）
      if (dnsR.status === "fulfilled") {
        const d = dnsR.value;
        const aligned =
          channel === "tunnel"
            ? d.kind === "alignedTunnel"
            : channel === "mesh"
              ? d.kind === "alignedMesh"
              : d.kind === "alignedDirect";
        items.push({
          label: t("tunnel.check.dns", lang),
          ok: aligned,
          detail: aligned ? okText : t("tunnel.checkupDnsHint", lang),
        });
      } else {
        items.push({ label: t("tunnel.check.dns", lang), ok: false, detail: failText });
      }

      // ② 网络归类（spec 002 知识：Public 下 443 规则不生效——仅影响直连入站，
      // 穿透流量为出站、组网访客经虚拟网络到达，均不受物理网络归类影响）
      if (channel === "tunnel") {
        items.push({
          label: t("tunnel.check.netCategory", lang),
          ok: null,
          detail: t("tunnel.check.netNaTunnel", lang),
        });
      } else if (channel === "mesh") {
        items.push({
          label: t("tunnel.check.netCategory", lang),
          ok: null,
          detail: t("tunnel.check.netNaMesh", lang),
        });
      } else {
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
      }

      // ③ 通道客户端（穿透判 frpc 状态；组网判服务/对端；直连不适用）
      if (channel === "tunnel" && tunR.status === "fulfilled") {
        const s = tunR.value.state;
        const ok = s === "online" || s === "starting";
        items.push({
          label: t("tunnel.check.tunnel", lang),
          ok,
          detail: t(tunnelStateKey(s), lang),
        });
      } else if (channel === "mesh") {
        if (meshR.status === "fulfilled") {
          const ms = meshR.value;
          const ok = ms.state === "online" || ms.state === "connecting";
          items.push({
            label: t("tunnel.check.mesh", lang),
            ok,
            detail:
              t(`mesh.state.${ms.state}` as DictKey, lang) +
              (ms.detail ? `（${meshDetailText(ms.detail, lang)}）` : ""),
          });
        }
      } else if (channel === "direct") {
        items.push({
          label: t("tunnel.check.tunnel", lang),
          ok: null,
          detail: t("tunnel.check.tunnelOff", lang),
        });
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

      // ⑥ 域名全链路（本机视角，spec 005 口径如实标注；组网态本机亦为成员，
      // 探测走虚拟 IP——外部可达性以成员设备实测为准）
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

      {/* 当前通道 + 切换入口（非现役通道各一按钮，mesh 推荐位在前） */}
      <div class="net__row">
        <span class="net__name">{t("tunnel.channelLabel", lang)}</span>
        <span class={`chip ${channel === "mesh" ? "chip--net-private" : channel === "tunnel" ? "chip--net-domain" : "chip--net-private"}`}>
          {t(channelKey(channel), lang)}
        </span>
        <span class="net__spacer" />
        {confirm === null
          ? others.map((ch) => (
              <span class="tunnel-switch" key={ch}>
                {disabledOf(ch) ? (
                  <span class="chip chip--stopped">{t("channel.disabled.disabledBadge", lang)}</span>
                ) : null}
                <button
                  class="btn btn--sm"
                  disabled={switching}
                  onClick={() => {
                    if (ch === "tunnel" && !configured) {
                      onToast(t("tunnel.notConfigured", lang), "error");
                      return;
                    }
                    setConfirm(ch);
                  }}
                >
                  {t(switchKey(ch), lang)}
                </button>
              </span>
            ))
          : null}
      </div>

      {/* 切换确认（风险/步骤先行，30s 自动还原；停用目标附重新启用安全警示） */}
      {confirm !== null ? (
        <div class="net__confirm">
          <p class="net__risk">
            <strong>{t(confirmTitleKey(confirm), lang)}</strong>
          </p>
          <p class="net__risk">
            {t(confirmStepsKey(confirm), lang).replace(
              "{ip}",
              meshCfg?.virtualIp ?? "",
            )}
          </p>
          {disabledOf(confirm) ? (
            <p class="net__risk">{t("channel.disabled.reenableRisk", lang)}</p>
          ) : null}
          <button class="btn btn--sm btn--primary" disabled={switching} onClick={() => void doSwitch(confirm)}>
            {switching ? t("tunnel.switching", lang) : t("tunnel.confirm", lang)}
          </button>
          <button class="btn btn--sm" disabled={switching} onClick={() => setConfirm(null)}>
            {t("tunnel.cancel", lang)}
          </button>
        </div>
      ) : null}

      {/* 隧道状态行（右侧：重新检测；穿透现役时附重启隧道） */}
      <div class="net__row">
        <span class="net__name">{t("tunnel.statusLabel", lang)}</span>
        {state ? (
          <>
            <span class={`chip ${tunnelChipClass(state)}`}>
              {t(tunnelStateKey(state), lang)}
            </span>
            {tunnelStatus?.detail ? (
              <span class="settings__desc">{tunnelDetailText(tunnelStatus.detail, lang)}</span>
            ) : null}
          </>
        ) : (
          <span class="muted">—</span>
        )}
        <span class="net__spacer" />
        <button class="btn btn--sm" disabled={dnsChecking} onClick={checkDns}>
          {dnsChecking ? t("tunnel.dnsChecking", lang) : t("tunnel.dnsRecheck", lang)}
        </button>
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

      {/* 组网状态区（现役 mesh 时呈现；状态 + 失败摘要 + 成员列表） */}
      {channel === "mesh" ? (
        <>
          <div class="net__row">
            <span class="net__name">{t("mesh.statusLabel", lang)}</span>
            {mstate ? (
              <>
                <span class={`chip ${meshChipClass(mstate)}`}>
                  {t(`mesh.state.${mstate}` as DictKey, lang)}
                </span>
                {meshStatus?.detail ? (
                  <span class="settings__desc">{meshDetailText(meshStatus.detail, lang)}</span>
                ) : null}
              </>
            ) : (
              <span class="muted">—</span>
            )}
            <span class="net__spacer" />
            <button class="btn btn--sm" disabled={applying} onClick={() => void applyMesh()}>
              {applying ? t("mesh.applying", lang) : t("mesh.apply", lang)}
            </button>
          </div>
          {meshStatus && meshStatus.peers.length > 0 ? (
            <div class="checkup__list">
              <span class="muted">{t("mesh.peersLabel", lang)}</span>
              {meshStatus.peers.map((p, i) => (
                <div class="checkup__item" key={`${p.hostname}-${i}`}>
                  <span class={`checkup__icon ${p.isLocal ? "is-na" : "is-ok"}`}>
                    {p.isLocal ? "·" : "✓"}
                  </span>
                  <span class="checkup__label">
                    {p.hostname}
                    {p.isLocal ? (
                      <span class="chip chip--stopped">{t("mesh.peerLocal", lang)}</span>
                    ) : null}
                  </span>
                  <span class="checkup__detail">
                    {p.ipv4 ?? "—"}
                    {p.latencyMs != null ? ` · ${Math.round(p.latencyMs)}ms` : ""}
                    {p.lossRate != null ? ` · ${(p.lossRate * 100).toFixed(1)}%` : ""}
                  </span>
                </div>
              ))}
            </div>
          ) : meshStatus && (mstate === "online" || mstate === "connecting") ? (
            <p class="muted">{t("mesh.noPeers", lang)}</p>
          ) : null}
        </>
      ) : null}

      {/* DNS 指引：仅异常时显示，对齐后自动隐藏（需求方 2026-09-10） */}
      {dns ? (
        <DnsNotice
          dns={dns}
          channel={channel}
          nodeDomain={tunnelCfg?.nodeDomain ?? ""}
          virtualIp={meshCfg?.virtualIp ?? ""}
          lang={lang}
        />
      ) : null}

      {/* 通道体检（复用 DNS/归类/通道/组件/全链路检查知识） */}
      <div class="checkup__head">
        <span class="muted">{t("tunnel.checkup", lang)}</span>
        <button class="btn btn--sm" disabled={checking} onClick={() => void runCheckup()}>
          {checking ? t("tunnel.checkupRunning", lang) : t("tunnel.checkupRun", lang)}
        </button>
      </div>
      {checkup ? (
        <div class="checkup__list">
          {checkup.map((item) => (
            <div class="checkup__item" key={item.label}>
              <span class={`checkup__icon ${item.ok === null ? "is-na" : item.ok ? "is-ok" : "is-bad"}`}>
                {item.ok === null ? "—" : item.ok ? "✓" : "✕"}
              </span>
              <span class="checkup__label">{item.label}</span>
              {item.detail ? <span class="checkup__detail">{item.detail}</span> : null}
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}

/** DNS 结论 → 提示条（仅异常时显示；对齐且通道配对 → null 自动隐藏，需求方 2026-09-10）。
 * 「对齐」判定与当前通道配对才算完成：穿透通道下残留 A 记录 = 待切换，反之亦然；
 * 组网态判 A=虚拟 IP（007），CNAME 残留按旁路暴露面提示。 */
function DnsNotice(props: {
  dns: DnsAlignment;
  channel: AccessChannel;
  nodeDomain: string;
  virtualIp: string;
  lang: Lang;
}) {
  const { dns, channel, nodeDomain, virtualIp, lang } = props;
  switch (dns.kind) {
    case "alignedTunnel":
      if (channel === "tunnel") return null;
      return (
        <p class="notice notice--warn">
          {t("tunnel.dnsGuideDirect", lang).replace("{target}", nodeDomain)}
        </p>
      );
    case "alignedDirect":
      if (channel === "direct") return null;
      return (
        <p class="notice notice--warn">
          {t("tunnel.dnsGuideTunnel", lang).replace("{target}", nodeDomain)}
        </p>
      );
    case "alignedMesh":
      if (channel === "mesh") return null;
      return (
        <p class="notice notice--warn">
          {t("tunnel.dnsGuideFromMesh", lang).replace("{ip}", virtualIp)}
        </p>
      );
    case "mismatchedCname":
      // 组网态的 CNAME 是旁路暴露面（应删），穿透态才是「目标不符」（应改值）
      if (channel === "mesh") {
        return (
          <p class="notice notice--warn">
            {t("tunnel.dnsCnameLeftMesh", lang).replace("{actual}", dns.actual)}
          </p>
        );
      }
      return (
        <p class="notice notice--warn">
          {t("tunnel.dnsMismatch", lang)
            .replace("{actual}", dns.actual)
            .replace("{target}", nodeDomain)}
        </p>
      );
    case "mismatchedA":
      return (
        <p class="notice notice--warn">
          {t("tunnel.dnsMismatchA", lang)
            .replace("{actual}", dns.actual)
            .replace("{target}", virtualIp)}
        </p>
      );
    case "noRecord":
      return (
        <p class="notice notice--warn">
          {t(
            channel === "tunnel"
              ? "tunnel.dnsGuideTunnel"
              : channel === "mesh"
                ? "tunnel.dnsGuideMesh"
                : "tunnel.dnsGuideDirect",
            lang,
          ).replace("{target}", channel === "mesh" ? virtualIp : nodeDomain)}
        </p>
      );
    case "queryFailed":
      return <p class="notice notice--warn">{t("tunnel.dnsFailed", lang)}</p>;
  }
}

/** 通道 → 当前态 chip 词条键 */
function channelKey(ch: AccessChannel): DictKey {
  return ch === "direct"
    ? "tunnel.channelDirect"
    : ch === "tunnel"
      ? "tunnel.channelTunnel"
      : "tunnel.channelMesh";
}

/** 通道 → 切换按钮词条键 */
function switchKey(ch: AccessChannel): DictKey {
  return ch === "direct"
    ? "tunnel.switchToDirect"
    : ch === "tunnel"
      ? "tunnel.switchToTunnel"
      : "tunnel.switchToMesh";
}

/** 通道 → 切换确认标题词条键 */
function confirmTitleKey(ch: AccessChannel): DictKey {
  return ch === "direct"
    ? "tunnel.confirmTitleDirect"
    : ch === "tunnel"
      ? "tunnel.confirmTitleTunnel"
      : "tunnel.confirmTitleMesh";
}

/** 通道 → 切换确认步骤词条键（mesh 文案含 {ip} 占位） */
function confirmStepsKey(ch: AccessChannel): DictKey {
  return ch === "direct"
    ? "tunnel.confirmStepsDirect"
    : ch === "tunnel"
      ? "tunnel.confirmStepsTunnel"
      : "tunnel.confirmStepsMesh";
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

/** 组网五态 → chip 配色（online 绿 / connecting 黄 / offline 红 / 其余灰） */
function meshChipClass(state: MeshStateKind): string {
  switch (state) {
    case "online":
      return "chip--running";
    case "connecting":
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

/** detail 载荷有两种形态：Rust 侧稳定码（走词典）与自由文案（如重启原因）。
 * 词典未命中即视为自由文案原样显示——否则会把「会话无响应，已自动重启…」
 * 渲染成词条名（`t` 的未知键回退是键本身）。 */
function tunnelDetailText(detail: string, lang: Lang): string {
  const key = `tunnel.code.${detail}` as DictKey;
  const mapped = t(key, lang);
  return mapped === key ? detail : mapped;
}

/** 组网 detail 为稳定码（mesh.rs DETAIL_*，构造上不含密钥）→ 词典文案；
 * 未命中按自由文案原样显示（与 tunnelDetailText 同防御）。 */
function meshDetailText(detail: string, lang: Lang): string {
  const key = `mesh.code.${detail}` as DictKey;
  const mapped = t(key, lang);
  return mapped === key ? detail : mapped;
}
