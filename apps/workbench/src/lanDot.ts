/**
 * 地址区「局域网」行绿豆判定（spec 010 验收期 UI 打磨，第 4 项；纯函数零运行时
 * 依赖——仅类型导入，`node --test tests/` 可直跑，见 tests/lanDot.test.ts）。
 *
 * 绿/红二值（用户定案），与域名行心跳同色语义：绿 = 此路通，红 = 此路不通
 * （例外关闭的默认收口态也红，配「不可直访(已收口)」文字——需求方已知悉）。
 *
 * 绿 = 例外规则实况存在（exception 为 on/expired，即规则在）∧ 活动网络归类为专用；
 * 红 = 其余：off（关例外红）/ pending（已请求未生效，直访同样不通）/ 尚无探测数据 /
 * 无活动网络 / 含非专用归类。
 *
 * 多网卡口径与后端 public_blocks_exception 对齐（lan_guard.rs plan R5：例外生效
 * ∧ ∃公用活动网络 → 直访不生效提示）：任一非专用归类即红——公用下例外完全不生效，
 * 域/未知归类 Private 规则同样不命中；全专用才绿。
 */

import type { ExceptionState, NetCategory } from "./types";

/** 例外规则实况存在（on/expired 即规则在；pending=已请求未生效不算，off 不算） */
export function lanExceptionRulePresent(exception: ExceptionState | null): boolean {
  return exception?.state === "on" || exception?.state === "expired";
}

/** 绿豆判定：true = 绿（此路通）；false = 红（此路不通）。 */
export function lanDotOk(
  exception: ExceptionState | null,
  categories: readonly NetCategory[],
): boolean {
  return (
    lanExceptionRulePresent(exception) &&
    categories.length > 0 &&
    categories.every((c) => c === "private")
  );
}
