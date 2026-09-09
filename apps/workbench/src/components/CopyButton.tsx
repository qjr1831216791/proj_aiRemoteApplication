/** 通用复制按钮：点击后短暂显示"已复制"反馈（主界面地址区 / 设置页只读卡共用） */

import { useState } from "preact/hooks";
import { copyText } from "../api";
import { t, type Lang } from "../i18n";

export interface CopyButtonProps {
  text: string;
  lang: Lang;
  onToast?: (text: string, kind?: "info" | "success" | "error") => void;
}

export function CopyButton(props: CopyButtonProps) {
  const { text, lang, onToast } = props;
  const [copied, setCopied] = useState(false);
  return (
    <button
      class="btn btn--sm"
      onClick={async () => {
        if (await copyText(text)) {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        } else {
          onToast?.(t("toast.copyFailed", lang), "error");
        }
      }}
    >
      {copied ? t("common.copied", lang) : t("common.copy", lang)}
    </button>
  );
}
