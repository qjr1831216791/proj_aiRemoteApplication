/**
 * 低频操作区（T15：AC19/20）。
 * - 折叠区（默认收起）：装机/升级/HTTPS 两件套/客户端配置/ddns-go 管理页/工作台
 * - 脚本缺失 → 对应按钮禁用 + ScriptLocator 禁用原因透传（spec §4.5）
 * - 派发失败/UAC 拒绝 → run_tool 返回 Err，toast 明确提示不崩溃（AC20）
 */

import { useState } from "preact/hooks";
import { api } from "../api";
import { t, type DictKey, type Lang } from "../i18n";
import type { ScriptsAvailability, ToolKind } from "../types";

export interface ToolsSectionProps {
  lang: Lang;
  scripts: ScriptsAvailability | null;
  onToast: (text: string, kind?: "info" | "success" | "error") => void;
}

/** 单个工具定义（needsScripts = 依赖 sprint0 脚本目录） */
interface ToolDef {
  id: string;
  label: DictKey;
  desc: DictKey;
  needsScripts: boolean;
  run: () => Promise<void>;
}

export function ToolsSection(props: ToolsSectionProps) {
  const { lang, scripts, onToast } = props;
  const [open, setOpen] = useState(false);
  const [mirror, setMirror] = useState(false);
  const [pending, setPending] = useState<string | null>(null);
  // 可用性未知（加载中）按可用渲染，按钮点击仍有后端校验兜底
  const available = scripts?.available ?? true;

  const runTool = (kind: ToolKind, update: boolean) => api.runTool(kind, { update, mirror });

  const defs: ToolDef[] = [
    {
      id: "install_server",
      label: "tools.installServer",
      desc: "tools.installServerDesc",
      needsScripts: true,
      run: () => runTool("install_server", false),
    },
    {
      id: "update_cloudcli",
      label: "tools.updateCloudcli",
      desc: "tools.updateCloudcliDesc",
      needsScripts: true,
      run: () => runTool("install_server", true),
    },
    {
      id: "install_https",
      label: "tools.installHttps",
      desc: "tools.installHttpsDesc",
      needsScripts: true,
      run: () => runTool("install_https", false),
    },
    {
      id: "enable_https",
      label: "tools.enableHttps",
      desc: "tools.enableHttpsDesc",
      needsScripts: true,
      run: () => runTool("enable_https", false),
    },
    {
      id: "install_client",
      label: "tools.installClient",
      desc: "tools.installClientDesc",
      needsScripts: true,
      run: () => runTool("install_client", false),
    },
    {
      id: "ddns_admin",
      label: "tools.openDdnsAdmin",
      desc: "tools.openDdnsAdminDesc",
      needsScripts: false,
      run: () => api.openExternal("ddns_admin"),
    },
    {
      id: "open_workbench",
      label: "tools.openWorkbench",
      desc: "tools.openWorkbenchDesc",
      needsScripts: false,
      run: () => api.openExternal("workbench"),
    },
  ];

  const dispatch = async (def: ToolDef) => {
    setPending(def.id);
    try {
      await def.run();
      onToast(t("tools.dispatched", lang), "success");
    } catch (e) {
      onToast(`${t("toast.opFailed", lang)}: ${String(e)}`, "error");
    } finally {
      setPending(null);
    }
  };

  return (
    <section class="card tools">
      <button class="tools__toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        <h2 class="card__title">{t("tools.title", lang)}</h2>
        <span class={`tools__chev${open ? " tools__chev--open" : ""}`}>▸</span>
      </button>
      {open ? (
        <div class="tools__body">
          <p class="muted">{t("tools.hint", lang)}</p>
          {!available ? (
            <p class="notice notice--warn">
              {t("tools.scriptsUnavailable", lang)}
              <code class="notice__detail">{scripts?.reason}</code>
            </p>
          ) : null}
          <label class="tools__mirror">
            <input
              type="checkbox"
              checked={mirror}
              onChange={(e) => setMirror(e.currentTarget.checked)}
            />
            {t("tools.useMirror", lang)}
          </label>
          <div class="tools__grid">
            {defs.map((d) => (
              <div class="tool" key={d.id}>
                <button
                  class="btn"
                  disabled={(d.needsScripts && !available) || pending !== null}
                  onClick={() => dispatch(d)}
                >
                  {pending === d.id ? t("main.busy", lang) : t(d.label, lang)}
                </button>
                <p class="tool__desc">{t(d.desc, lang)}</p>
              </div>
            ))}
          </div>
          <p class="muted">{t("tools.uacHint", lang)}</p>
        </div>
      ) : null}
    </section>
  );
}
