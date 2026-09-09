import { useEffect, useState } from "preact/hooks";
import { detectLang, t, type Lang } from "./i18n";

/**
 * T1 骨架页：验证 蓝白主题 tokens + 双语 t() + 窗口就绪后显示（visible:false → show）。
 * 真实状态卡/总开关在 T13 实现。
 */
export function App() {
  const [lang, setLang] = useState<Lang>(() => detectLang());

  // 窗口以 visible:false 创建，页面就绪后显形（规避 WebView2 首帧白屏，plan §3.1）；
  // --hidden 启动（登录自启，T12/AC10）时保持隐藏：show 前经 Rust 侧查询门控
  useEffect(() => {
    if ((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) {
      Promise.all([
        import("@tauri-apps/api/core"),
        import("@tauri-apps/api/window"),
      ]).then(([{ invoke }, { getCurrentWindow }]) => {
        invoke<boolean>("is_hidden_startup").then((hidden) => {
          if (!hidden) getCurrentWindow().show();
        });
      });
    }
  }, []);

  return (
    <main class="app">
      <header class="app__header">
        <h1 class="app__title">{t("app.title", lang)}</h1>
        <p class="app__subtitle">{t("app.subtitle", lang)}</p>
      </header>

      <section class="card card--placeholder">
        <p>{t("app.scaffoldNote", lang)}</p>
      </section>

      <footer class="app__footer">
        <div class="lang-switch" role="group" aria-label="language">
          <button
            class={lang === "zh" ? "btn btn--sm btn--active" : "btn btn--sm"}
            onClick={() => setLang("zh")}
          >
            中文
          </button>
          <button
            class={lang === "en" ? "btn btn--sm btn--active" : "btn btn--sm"}
            onClick={() => setLang("en")}
          >
            English
          </button>
        </div>
      </footer>
    </main>
  );
}
