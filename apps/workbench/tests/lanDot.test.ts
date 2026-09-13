/**
 * 局域网绿豆判定单测（spec 010 验收期 UI 打磨第 4 项）。
 * 运行：cd apps/workbench && npm test（node 内建 test runner，零依赖——
 * 项目无 vitest 前端测试基建，纯 TS 模块由 node 类型剥离直跑）。
 * 覆盖：绿/红各分支 + 公用例外红 + 关例外红 + pending 红 + 无数据红。
 */

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { lanDotOk, lanExceptionRulePresent } from "../src/lanDot.ts";
import type { ExceptionState, NetCategory } from "../src/types.ts";

const OFF: ExceptionState = { state: "off" };
const ON: ExceptionState = { state: "on", remainingSecs: 3600 };
const EXPIRED: ExceptionState = { state: "expired" };
const PENDING: ExceptionState = { state: "pending" };

describe("lanExceptionRulePresent（例外规则实况存在）", () => {
  it("on / expired = 规则在", () => {
    assert.equal(lanExceptionRulePresent(ON), true);
    assert.equal(lanExceptionRulePresent(EXPIRED), true);
  });
  it("off / pending / 尚无探测(null) = 规则不在", () => {
    assert.equal(lanExceptionRulePresent(OFF), false);
    assert.equal(lanExceptionRulePresent(PENDING), false);
    assert.equal(lanExceptionRulePresent(null), false);
  });
});

describe("lanDotOk（绿 = 规则在 ∧ 活动网络全专用）", () => {
  it("绿：例外 on + 专用网络", () => {
    assert.equal(lanDotOk(ON, ["private"]), true);
  });
  it("绿：例外 expired（规则仍在）+ 专用网络", () => {
    assert.equal(lanDotOk(EXPIRED, ["private"]), true);
  });
  it("绿：多网卡全部专用", () => {
    assert.equal(lanDotOk(ON, ["private", "private"]), true);
  });
  it("关例外红：off + 专用网络（默认收口态，配「不可直访(已收口)」）", () => {
    assert.equal(lanDotOk(OFF, ["private"]), false);
  });
  it("公用例外红：on + 公用网络（例外在公用下不生效，与 public_blocks 对齐）", () => {
    assert.equal(lanDotOk(ON, ["public"]), false);
  });
  it("公用例外红：expired + 公用网络同样红", () => {
    assert.equal(lanDotOk(EXPIRED, ["public"]), false);
  });
  it("pending 红：已请求但规则未生效，直访不通", () => {
    assert.equal(lanDotOk(PENDING, ["private"]), false);
  });
  it("尚无探测数据红：lanHealth 为 null", () => {
    assert.equal(lanDotOk(null, ["private"]), false);
  });
  it("无活动网络红：无归类证据不给绿", () => {
    assert.equal(lanDotOk(ON, []), false);
  });
  it("域/未知归类红：Private 规则不命中", () => {
    assert.equal(lanDotOk(ON, ["domain"]), false);
    assert.equal(lanDotOk(ON, ["unknown"]), false);
  });
  it("混合归类红：任一公用即红（多网卡口径与后端 R5 对齐）", () => {
    assert.equal(lanDotOk(ON, ["private", "public"]), false);
  });
});
