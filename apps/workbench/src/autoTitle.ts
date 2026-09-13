/**
 * 按钮悬停全文提示（2026-09-13 需求方反馈）：窄窗口下按钮文字不折行
 * （`.btn { white-space: nowrap }`，2026-09-11 定），被压窄时文字看不全，
 * 而按钮本身没有 hover 查看全文的能力——这里全局自动补 `title`。
 *
 * 设计要点：
 * - **作者优先**：手写了 `title` 的按钮不被覆盖。区分手段是模块内的 WeakMap
 *   记账（记下每次自动写入的值），不往 DOM 里塞标记属性——将来某组件给按钮
 *   手写 `title` 时不会被本模块吃掉（值既非当前文案、也非上次自动写入的值
 *   即判定为作者手写）。
 * - **随文案刷新**：忙碌态会换词的按钮（如「加载中…」→「保存」）每次同步刷新
 *   提示，不留旧文案；文案变空则撤销提示并清账。
 * - **零侵入**：不进组件树、不改任何调用点；挂一次全局装配后，后续渲染出的
 *   新按钮（视图切换、模态弹出）自动纳入。
 */

/** 每个元素最后一次被本模块写入的 title（区分作者手写与自动写入） */
const lastSet = new WeakMap<Element, string>();

/**
 * 可见文案 → 提示文本（纯函数，供单测）：压平换行/多空格，空文案返回 null
 * （图标按钮或纯空白文本不给提示）。
 */
export function autoTitleText(raw: string | null | undefined): string | null {
  const text = (raw ?? "").replace(/\s+/g, " ").trim();
  return text.length === 0 ? null : text;
}

/** 该元素是否带着「作者手写」的 title（有 title 且不是我们上次写的那个值） */
function hasAuthoredTitle(el: Element): boolean {
  if (!el.hasAttribute("title")) return false;
  return el.getAttribute("title") !== lastSet.get(el);
}

/** 给 root 下所有按钮补齐/刷新 title（幂等；作者 title 不动） */
export function fillButtonTitles(root: ParentNode): void {
  for (const el of root.querySelectorAll("button")) {
    if (hasAuthoredTitle(el)) continue;
    const text = autoTitleText(el.textContent);
    if (text === null) {
      if (lastSet.has(el)) {
        el.removeAttribute("title");
        lastSet.delete(el);
      }
      continue;
    }
    if (el.getAttribute("title") !== text) el.setAttribute("title", text);
    lastSet.set(el, text);
  }
}

/**
 * 装配：立即补一遍，并监听后续 DOM 变化（视图切换、模态弹出、文案换词）。
 * 变化按 rAF 合帧，避免连渲染时反复全量扫描。
 * @returns 卸载函数（组件卸载时断开观察）
 */
export function installAutoTitles(root: ParentNode = document.body): () => void {
  fillButtonTitles(root);
  let queued = false;
  const schedule = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      fillButtonTitles(root);
    });
  };
  const mo = new MutationObserver(schedule);
  // 只观察结构/文本：本模块只写属性，不触发自身回调（无自激循环）
  mo.observe(root, { childList: true, subtree: true, characterData: true });
  return () => mo.disconnect();
}
