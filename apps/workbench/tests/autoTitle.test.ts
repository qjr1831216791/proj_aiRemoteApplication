/**
 * 按钮悬停全文提示单测（2026-09-13 需求方反馈）。
 * 运行：cd apps/workbench && npm test（node 内建 test runner，零依赖）。
 * DOM 部分用最小桩（只实现 fillButtonTitles 真正调用的那几个方法），
 * 不引入 jsdom——覆盖：文案压平、作者 title 优先、随文案刷新、文案变空撤销。
 */

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { autoTitleText, fillButtonTitles } from "../src/autoTitle.ts";

// ── 最小 DOM 桩 ───────────────────────────────────────────────────────────────

interface FakeBtn {
  textContent: string;
  attrs: Record<string, string>;
}

function mkBtn(text: string, attrs: Record<string, string> = {}): FakeBtn & {
  hasAttribute(n: string): boolean;
  getAttribute(n: string): string | null;
  setAttribute(n: string, v: string): void;
  removeAttribute(n: string): void;
} {
  const btn = {
    textContent: text,
    attrs: { ...attrs },
    hasAttribute(n: string) {
      return n in btn.attrs;
    },
    getAttribute(n: string) {
      return n in btn.attrs ? btn.attrs[n] : null;
    },
    setAttribute(n: string, v: string) {
      btn.attrs[n] = v;
    },
    removeAttribute(n: string) {
      delete btn.attrs[n];
    },
  };
  return btn;
}

function mkRoot(btns: unknown[]): ParentNode {
  return { querySelectorAll: () => btns } as unknown as ParentNode;
}

// ── autoTitleText（纯函数）────────────────────────────────────────────────────

describe("autoTitleText", () => {
  it("压平换行与连续空格（多行标签折成一行提示）", () => {
    assert.equal(autoTitleText("  写入组网\n  密钥 "), "写入组网 密钥");
  });
  it("普通文案原样返回", () => {
    assert.equal(autoTitleText("修复白名单"), "修复白名单");
  });
  it("空串 / 纯空白 / null / undefined → null（图标按钮不给提示）", () => {
    assert.equal(autoTitleText(""), null);
    assert.equal(autoTitleText("   \n\t "), null);
    assert.equal(autoTitleText(null), null);
    assert.equal(autoTitleText(undefined), null);
  });
});

// ── fillButtonTitles（DOM 行为）───────────────────────────────────────────────

describe("fillButtonTitles", () => {
  it("无 title 的按钮：补上可见文案", () => {
    const btn = mkBtn("开启例外…");
    fillButtonTitles(mkRoot([btn]));
    assert.equal(btn.getAttribute("title"), "开启例外…");
  });

  it("作者手写的 title 不被覆盖", () => {
    const btn = mkBtn("诊断", { title: "逐对端探测（最长约数秒）" });
    fillButtonTitles(mkRoot([btn]));
    assert.equal(btn.getAttribute("title"), "逐对端探测（最长约数秒）");
  });

  it("幂等：文案未变则重复调用不改写", () => {
    const btn = mkBtn("保存");
    const root = mkRoot([btn]);
    fillButtonTitles(root);
    fillButtonTitles(root);
    assert.equal(btn.getAttribute("title"), "保存");
  });

  it("外部写入的 title 被尊重（不再被自动值覆盖）", () => {
    const btn = mkBtn("保存");
    const root = mkRoot([btn]);
    fillButtonTitles(root);
    btn.setAttribute("title", "保存当前组网配置"); // 作者后置手写
    fillButtonTitles(root);
    assert.equal(btn.getAttribute("title"), "保存当前组网配置");
  });

  it("忙碌态换词：提示随文案刷新", () => {
    const btn = mkBtn("加载中…");
    const root = mkRoot([btn]);
    fillButtonTitles(root);
    assert.equal(btn.getAttribute("title"), "加载中…");
    btn.textContent = "保存";
    fillButtonTitles(root);
    assert.equal(btn.getAttribute("title"), "保存");
  });

  it("文案变空：撤销本模块打的提示（不留旧文案）", () => {
    const btn = mkBtn("同步 DNS");
    const root = mkRoot([btn]);
    fillButtonTitles(root);
    btn.textContent = "";
    fillButtonTitles(root);
    assert.equal(btn.getAttribute("title"), null);
  });

  it("空文案按钮：始终不给提示", () => {
    const btn = mkBtn("   ");
    fillButtonTitles(mkRoot([btn]));
    assert.equal(btn.getAttribute("title"), null);
  });
});
