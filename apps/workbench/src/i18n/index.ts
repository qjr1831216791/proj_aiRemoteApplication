import { zh, type DictKey } from "./zh";
import { en } from "./en";

/**
 * 轻量双语词典（plan §2：零依赖 i18n，与 sprint0 脚本的 T() 同构）。
 * 生效语言 = 显式设置优先；auto = 系统显示语言 zh* → 中文，否则英文（AC25）。
 */
export type Lang = "zh" | "en";
export type LangSetting = "auto" | Lang;

const dicts: Record<Lang, Record<DictKey, string>> = { zh, en };

/** auto 判定（前端侧 navigator.language；Rust 侧另有同语义实现供脚本 -Lang 使用） */
export function resolveLang(setting: LangSetting): Lang {
  if (setting !== "auto") return setting;
  return detectLang();
}

/** 系统显示语言探测：zh* → zh，其余 → en */
export function detectLang(): Lang {
  return /^zh/i.test(navigator.language) ? "zh" : "en";
}

export function t(key: DictKey, lang: Lang): string {
  return dicts[lang][key] ?? dicts.en[key] ?? key;
}

export type { DictKey };
