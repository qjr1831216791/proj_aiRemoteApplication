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
 * - 访问白名单（spec 010 T6）：健康 chip（正常/休眠/待修复）+ 失配「修复白名单」
 *   + 旧规则迁移横幅「一键收口」+ 3001 例外开关（风险确认模态含归类前提 + 12h 回落如实呈现）
 * - 通道体检：DNS / 网络归类（组网不适用）/ 组网客户端 / 本机组件 / 域名全链路
 * 通道与配置数据源：settings（App 持有）；白名单健康：languard://changed 事件
 * （60s 监视 + 动作后即时复测经 MainView 下沉的回调）。
 */

import { useEffect, useState } from "preact/hooks";
import { api } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type {
  DnsAlignment,
  LanHealth,
  MeshStateKind,
  MeshStatus,
  Settings,
  WhitelistState,
} from "../types";
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
  /** 白名单健康快照（spec 010；null = 尚无成功探测；MainView 持有） */
  lanHealth: LanHealth | null;
  /** 白名单动作后的即时复测（延迟追加由本组件排程） */
  onLanRefresh: () => void;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
}

/** DNS 检测周期（常态轮询；对齐仅隐藏指引，轮询持续以便感知记录漂移） */
const DNS_CHECK_INTERVAL = 30_000;

export function MeshCard(props: MeshCardProps) {
  const { lang, settings, meshStatus, lanHealth, onLanRefresh, onToast } = props;
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

  // ── 访问白名单（spec 010 T6）───────────────────────────────────────────

  /** 例外开启的风险确认（30s 未确认自动收起，沿网络归类切换确认先例） */
  const [confirmExc, setConfirmExc] = useState(false);
  useEffect(() => {
    if (!confirmExc) return;
    const id = setTimeout(() => setConfirmExc(false), 30000);
    return () => clearTimeout(id);
  }, [confirmExc]);

  /** 白名单动作统一收口：成功 toast + 即时/延迟复测（UAC 窗内规则数秒后才落位，
   * 沿归类切换 3.5s 追加复测先例，12s 二次兜底；60s 监视器轮询兜尾）；
   * 失败 toast（AC7：UAC 拒绝 → Err，状态原样不崩溃） */
  const runLan = async (key: string, action: () => Promise<unknown>) => {
    setBusy(key);
    try {
      await action();
      onToast(t("tools.dispatched", lang), "success");
      onLanRefresh();
      setTimeout(onLanRefresh, 3500);
      setTimeout(onLanRefresh, 12000);
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setBusy(null);
    }
  };

  /** 例外四态 → chip（on 橙警 + 剩余时长 / expired·pending 红 / off 灰） */
  const exc = lanHealth?.exception ?? null;
  const excChip = !exc ? (
    <span class="muted">—</span>
  ) : exc.state === "on" ? (
    <span class="chip chip--net-public">
      {t("languard.exceptionRemaining", lang).replace(
        "{h}",
        String(Math.ceil(exc.remainingSecs / 3600)),
      )}
    </span>
  ) : exc.state === "expired" ? (
    <span class="chip chip--failed">{t("languard.exceptionExpired", lang)}</span>
  ) : exc.state === "pending" ? (
    <span class="chip chip--failed">{t("languard.exceptionPending", lang)}</span>
  ) : (
    <span class="chip chip--stopped">{t("languard.excOffChip", lang)}</span>
  );

  /** 例外动作：on/expired → 关闭（off 动作同开关，expired 即手动回落）；
   * off/pending → 开启（pending 重开即重新派发）；无探测数据不出现按钮 */
  const excBtn =
    !exc ? null : exc.state === "on" || exc.state === "expired" ? (
      <button
        class="btn btn--sm"
        disabled={busy !== null}
        onClick={() => void runLan("excOff", () => api.lanGuardSetException(false))}
      >
        {t("languard.exceptionOffBtn", lang)}
      </button>
    ) : (
      <button class="btn btn--sm" disabled={busy !== null} onClick={() => setConfirmExc(true)}>
        {t("languard.exceptionOnBtn", lang)}
      </button>
    );

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

      {/* 访问白名单（spec 010 T6）：健康 chip + 失配修复；数据源 languard://changed
          （后端 60s 监视）+ 动作后即时复测 */}
      <div class="net__row">
        <span class="net__name">{t("languard.title", lang)}</span>
        {lanHealth ? (
          <>
            <span class={`chip ${wlChipClass(lanHealth.whitelist)}`}>
              {t(wlChipKey(lanHealth.whitelist), lang)}
            </span>
            <span class="settings__desc">{t(wlHintKey(lanHealth.whitelist), lang)}</span>
          </>
        ) : (
          <span class="muted">—</span>
        )}
        <span class="net__spacer" />
        {lanHealth && wlNeedsFix(lanHealth.whitelist) ? (
          <button
            class="btn btn--sm"
            disabled={busy !== null}
            onClick={() => void runLan("wlFix", () => api.lanGuardEnsureWhitelist())}
          >
            {t("languard.fixBtn", lang)}
          </button>
        ) : null}
      </div>

      {/* 旧规则迁移横幅（AC10）：一键收口 = 幂等删两旧规则 + 就位白名单（端到端演练 T7） */}
      {lanHealth?.legacyPresent ? (
        <div class="net__confirm">
          <p class="net__risk">{t("languard.legacyBanner", lang)}</p>
          <button
            class="btn btn--sm btn--primary"
            disabled={busy !== null}
            onClick={() => void runLan("migrate", () => api.lanGuardMigrate())}
          >
            {t("languard.migrateBtn", lang)}
          </button>
        </div>
      ) : null}

      {/* 例外开关（AC6/AC7/AC8）：开启走风险确认模态；开启中显示剩余时长 */}
      <div class="net__row">
        <span class="net__name">{t("languard.exceptionLabel", lang)}</span>
        {excChip}
        <span class="net__spacer" />
        {excBtn}
      </div>
      {lanHealth?.publicBlocksException ? (
        <p class="notice notice--warn">{t("languard.publicBlocks", lang)}</p>
      ) : null}
      {confirmExc ? (
        <div class="net__confirm">
          <p class="net__risk">
            {t("languard.riskTitle", lang)}
            <br />
            {t("languard.riskBody", lang)}
            <br />
            {t("languard.riskTtl", lang)}
            <br />
            {/* 归类前提（spec 010 验收期文案回填）：例外 × 网络归类两把锁——
                仅「例外开 + 专用」才放行，公用下例外完全不生效 */}
            {t("languard.riskProfile", lang)}
          </p>
          <button
            class="btn btn--sm btn--primary"
            disabled={busy !== null}
            onClick={() => {
              setConfirmExc(false);
              void runLan("excOn", () => api.lanGuardSetException(true));
            }}
          >
            {t("languard.riskConfirm", lang)}
          </button>
          <button class="btn btn--sm" onClick={() => setConfirmExc(false)}>
            {t("net.cancel", lang)}
          </button>
        </div>
      ) : null}

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
          {/* 推荐客户端（spec 009 变更 2026-09-11 T14）：仅推荐 EasyTier 官方客户端；
              第三方 Orbit 因移动端隧道在系统网络切换/重启后易失效已撤销推荐 */}
          <p class="muted">
            {t("mesh.memberClientRec", lang)}{" "}
            <a
              class="mesh-member__link"
              href="https://github.com/EasyTier/EasyTier/releases"
              target="_blank"
              rel="noreferrer"
            >
              GitHub Releases
            </a>
          </p>
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

/** 白名单五态 → chip 配色（纯函数：ok 绿 / dormant 灰（休眠非异常）/ 失配红） */
function wlChipClass(state: WhitelistState): string {
  switch (state) {
    case "ok":
      return "chip--running";
    case "dormant":
      return "chip--stopped";
    default:
      return "chip--failed";
  }
}

/** 白名单五态 → chip 三分类键（正常 / 休眠 / 待修复） */
function wlChipKey(state: WhitelistState): DictKey {
  switch (state) {
    case "ok":
      return "languard.wl.ok";
    case "dormant":
      return "languard.wl.dormant";
    default:
      return "languard.wl.fix";
  }
}

/** 白名单五态 → 提示行键（chip 同行的如实说明） */
function wlHintKey(state: WhitelistState): DictKey {
  switch (state) {
    case "ok":
      return "languard.wl.okHint";
    case "dormant":
      return "languard.wl.dormantHint";
    default:
      return "languard.wl.fixHint";
  }
}

/** 待修复判定（missing / staleCidr / staleIface → 「修复白名单」按钮消费） */
function wlNeedsFix(state: WhitelistState): boolean {
  return state === "missing" || state === "staleCidr" || state === "staleIface";
}

/** 组网 detail 为稳定码（mesh.rs DETAIL_*，构造上不含密钥）→ 词典文案；
 * 未命中按自由文案原样显示（t 的未知键回退是键本身）。 */
function meshDetailText(detail: string, lang: Lang): string {
  const key = `mesh.code.${detail}` as DictKey;
  const mapped = t(key, lang);
  return mapped === key ? detail : mapped;
}
