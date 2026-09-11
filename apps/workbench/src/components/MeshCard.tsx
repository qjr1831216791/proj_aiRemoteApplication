/**
 * 组网通道卡（spec 008 T15/D5：由三通道 TunnelCard 重构，组网单通道）。
 * - 概览行：虚拟 IP（一键复制，成员入网即用）+ 在线成员数（mesh_status 数据源）
 * - 组网状态区：mesh://status 四态 + detail 稳定码 + 成员列表（isLocal 标注；
 *   在线判定在 Rust 侧排除本机项）
 * - 常驻操作（向导组网分支同款能力下沉）：写入组网密钥 / 安装·刷新服务 /
 *   同步 DNS（CNAME 全删 + A → 虚拟 IP）——装机后日常维护不再依赖向导
 * - 成员入网配置（spec 009 US4）：折叠区展示官方 TOML 对照清单（密钥占位
 *   符 + 指引文案），移动端 App 逐项输入避免漏项错配
 * - DNS 指引：常态轮询权威检测，对齐即消失；判 A=虚拟 IP，
 *   CNAME 残留按旁路暴露面提示（spec 007 体检口径延续）
 * - 通道体检：DNS / 网络归类（组网不适用）/ 组网客户端 / 本机组件 / 域名全链路
 * 通道与配置数据源：settings（App 持有）。
 */

import { useEffect, useState } from "preact/hooks";
import { api } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type { DnsAlignment, MeshStateKind, MeshStatus, Settings } from "../types";
import { CopyButton } from "./CopyButton";

/** 体检单项结论 */
interface CheckItem {
  label: string;
  /** true=正常 false=异常 null=不适用（组网模式下的网络归类项） */
  ok: boolean | null;
  detail?: string;
}

export interface MeshCardProps {
  lang: Lang;
  settings: Settings | null;
  meshStatus: MeshStatus | null;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
}

/** DNS 检测周期（常态轮询；对齐仅隐藏指引，轮询持续以便感知记录漂移） */
const DNS_CHECK_INTERVAL = 30_000;

export function MeshCard(props: MeshCardProps) {
  const { lang, settings, meshStatus, onToast } = props;
  const meshCfg = settings?.mesh ?? null;

  // DNS 对齐检测：常态轮询（A=虚拟 IP 生效需常态盯；对齐即隐藏指引）
  const [dns, setDns] = useState<DnsAlignment | null>(null);
  const [dnsChecking, setDnsChecking] = useState(false);
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
    checkDns();
    const id = setInterval(checkDns, DNS_CHECK_INTERVAL);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 通道体检（聚合 DNS/归类/组网客户端/本机组件/域名全链路）
  const [checkup, setCheckup] = useState<CheckItem[] | null>(null);
  const [checking, setChecking] = useState(false);
  // 在途动作键（防重复点击：应用/装服务/密钥/同步 DNS）
  const [busy, setBusy] = useState<string | null>(null);

  // 成员入网配置（spec 009 US4）：折叠区，每次展开都重新拉取
  //（组网设置可能已变，旧缓存无意义；命令纯内存拼装，开销可忽略）
  const [memberCfgOpen, setMemberCfgOpen] = useState(false);
  const [memberCfg, setMemberCfg] = useState<string | null>(null);
  const [memberCfgErr, setMemberCfgErr] = useState<string | null>(null);
  const openMemberCfg = (open: boolean) => {
    setMemberCfgOpen(open);
    if (open) {
      setMemberCfg(null);
      setMemberCfgErr(null);
      api
        .meshMemberConfig()
        .then(setMemberCfg)
        .catch((e) => setMemberCfgErr(String(e)));
    }
  };

  /** 脚本/命令派发统一收口（沿向导 dispatch 模式） */
  const dispatch = async (key: string, action: () => Promise<unknown>, doneHint = false) => {
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

  /** 同步 DNS（AC13）：删残留 CNAME + A upsert 虚拟 IP；操作数透出 */
  const syncDns = async () => {
    setBusy("syncDns");
    try {
      const n = await api.meshSyncDns();
      setDns(null);
      onToast(t("mesh.syncDnsDone", lang).replace("{n}", String(n)), "success");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setBusy(null);
    }
  };

  /** 通道体检（组网单通道口径；组网访客经虚拟网络到达，物理网络归类不影响） */
  const runCheckup = async () => {
    setChecking(true);
    try {
      const [dnsR, meshR, compR, healthR] = await Promise.allSettled([
        api.checkDnsAlignment(),
        api.getMeshStatus(),
        api.getStatus(),
        api.checkDomainHealthNow(),
      ]);
      const items: CheckItem[] = [];
      const okText = t("tunnel.check.ok", lang);
      const failText = t("tunnel.check.fail", lang);

      // ① DNS 解析对齐（权威口径，与上方指引同源；A=虚拟 IP）
      if (dnsR.status === "fulfilled") {
        const aligned = dnsR.value.kind === "alignedMesh";
        items.push({
          label: t("tunnel.check.dns", lang),
          ok: aligned,
          detail: aligned ? okText : t("tunnel.checkupDnsHint", lang),
        });
      } else {
        items.push({ label: t("tunnel.check.dns", lang), ok: false, detail: failText });
      }

      // ② 网络归类（组网访客经虚拟网络到达，不受物理网络归类影响——不适用）
      items.push({
        label: t("tunnel.check.netCategory", lang),
        ok: null,
        detail: t("tunnel.check.netNaMesh", lang),
      });

      // ③ 组网客户端（服务/对端实况）
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
      } else {
        items.push({ label: t("tunnel.check.mesh", lang), ok: false, detail: failText });
      }

      // ④⑤ 本机组件（caddy 443 / 上游 3001，spec 001 probe 快照）
      if (compR.status === "fulfilled") {
        for (const id of ["caddy", "cloudcli"] as const) {
          const c = compR.value.find((s) => s.id === id);
          const running = c?.state === "running";
          // port-held 是 kebab、词典键为 camel（common.portHeld），与 MainView 同款特判
          const stateKey: DictKey =
            c?.state === "port-held" ? "common.portHeld" : `common.${c?.state ?? "stopped"}`;
          items.push({
            label: t(id === "caddy" ? "tunnel.check.caddy" : "tunnel.check.upstream", lang),
            ok: running,
            detail: running ? okText : t(stateKey, lang),
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

  const mstate: MeshStateKind | null = meshStatus?.state ?? null;
  const peersOnline = meshStatus
    ? meshStatus.peers.filter((p) => !p.isLocal).length
    : null;

  return (
    <section class="card">
      <h2 class="card__title">{t("mesh.cardTitle", lang)}</h2>

      {/* 概览行：虚拟 IP（复制入网即用）+ 在线成员数 */}
      <div class="net__row">
        <span class="net__name">{t("mesh.virtualIpLabel", lang)}</span>
        <code class="wizard__url">{meshCfg?.virtualIp ?? "—"}</code>
        {meshCfg?.virtualIp ? (
          <CopyButton text={meshCfg.virtualIp} lang={lang} onToast={onToast} />
        ) : null}
        <span class="net__spacer" />
        <span class="net__name">{t("mesh.peersOnlineLabel", lang)}</span>
        {peersOnline === null ? (
          <span class="muted">—</span>
        ) : (
          <span class={`chip ${peersOnline > 0 ? "chip--running" : "chip--stopped"}`}>
            {peersOnline}
          </span>
        )}
      </div>

      {/* 组网状态行：状态 + 失败摘要；右侧维护入口（重新检测 DNS / 应用配置 / 安装·刷新服务） */}
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
        <button class="btn btn--sm" disabled={dnsChecking} onClick={checkDns}>
          {dnsChecking ? t("tunnel.dnsChecking", lang) : t("tunnel.dnsRecheck", lang)}
        </button>
        <button
          class="btn btn--sm"
          disabled={busy !== null}
          onClick={() => dispatch("apply", () => api.meshApplyConfig(), false)}
        >
          {busy === "apply" ? t("mesh.applying", lang) : t("mesh.apply", lang)}
        </button>
        <button
          class="btn btn--sm"
          disabled={busy !== null}
          onClick={() => dispatch("install", () => api.meshInstallService(), true)}
        >
          {t("mesh.installBtn", lang)}
        </button>
      </div>

      {/* 成员列表（peer list 首项恒为本机，isLocal 标注） */}
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

      {/* DNS 指引：仅异常时显示，对齐后自动隐藏 */}
      {dns ? <DnsNotice dns={dns} virtualIp={meshCfg?.virtualIp ?? ""} lang={lang} /> : null}

      {/* 常驻操作（向导同款能力下沉）：密钥写入 / 同步 DNS */}
      <div class="master__actions">
        <button
          class="btn btn--sm"
          disabled={busy !== null}
          onClick={() =>
            dispatch(
              "secret",
              () => api.runTool("set_mesh_secret", { update: false, mirror: false, domain: null }),
              true,
            )
          }
        >
          {t("mesh.secretBtn", lang)}
        </button>
        <button class="btn btn--sm" disabled={busy !== null} onClick={() => void syncDns()}>
          {busy === "syncDns" ? t("tunnel.dnsChecking", lang) : t("mesh.syncDnsBtn", lang)}
        </button>
      </div>
      <p class="muted">{t("mesh.syncDnsHint", lang)}</p>

      {/* 成员入网配置（spec 009 US4）：折叠区默认收起，展开拉取；
          密钥为占位符 + 指引文案，真实密钥不出现（spec 007 AC8 延续） */}
      <button
        class="tools__toggle"
        aria-expanded={memberCfgOpen}
        onClick={() => openMemberCfg(!memberCfgOpen)}
      >
        <h3 class="card__title mesh-member__title">{t("mesh.memberConfig", lang)}</h3>
        <span class={`tools__chev${memberCfgOpen ? " tools__chev--open" : ""}`}>▸</span>
      </button>
      {memberCfgOpen ? (
        <div class="tools__body">
          <p class="muted">{t("mesh.memberConfigHint", lang)}</p>
          {memberCfgErr ? (
            <p class="notice notice--warn">{memberCfgErr}</p>
          ) : memberCfg === null ? (
            <p class="muted">{t("common.loading", lang)}</p>
          ) : (
            <>
              <div class="mesh-member__actions">
                <CopyButton text={memberCfg} lang={lang} onToast={onToast} />
              </div>
              <pre class="mesh-member__pre">{memberCfg}</pre>
            </>
          )}
        </div>
      ) : null}

      {/* 通道体检（复用 DNS/归类/组网/组件/全链路检查知识） */}
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

/** DNS 结论 → 提示条（仅异常时显示；A=虚拟 IP 对齐 → null 自动隐藏）。
 * 残留 CNAME = 旁路暴露面（应删，走「同步 DNS」）；A 值不符 = 待改值。 */
function DnsNotice(props: { dns: DnsAlignment; virtualIp: string; lang: Lang }) {
  const { dns, virtualIp, lang } = props;
  switch (dns.kind) {
    case "alignedMesh":
      return null;
    case "mismatchedCname":
      return (
        <p class="notice notice--warn">
          {t("tunnel.dnsCnameLeftMesh", lang).replace("{actual}", dns.actual)}
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
          {t("tunnel.dnsGuideMesh", lang).replace("{target}", virtualIp)}
        </p>
      );
    case "queryFailed":
      return <p class="notice notice--warn">{t("tunnel.dnsFailed", lang)}</p>;
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

/** 组网 detail 为稳定码（mesh.rs DETAIL_*，构造上不含密钥）→ 词典文案；
 * 未命中按自由文案原样显示（t 的未知键回退是键本身）。 */
function meshDetailText(detail: string, lang: Lang): string {
  const key = `mesh.code.${detail}` as DictKey;
  const mapped = t(key, lang);
  return mapped === key ? detail : mapped;
}
