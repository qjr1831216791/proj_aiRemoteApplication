/** 中文词条（与 en.ts 键集严格一致；新增键必须两边同步） */
export const zh = {
  "app.title": "AI 远程工作台",
  "app.subtitle": "手机遥控开发机上的 Claude Code",
  "app.scaffoldNote": "工程骨架已就绪（T1）——状态卡与总开关将在后续任务接入。",
  "common.start": "启动",
  "common.stop": "停止",
  "common.retry": "重试",
  "common.running": "运行中",
  "common.stopped": "未运行",
  "common.starting": "启动中",
  "common.failed": "失败",
  "common.portHeld": "端口被占",
} as const;

export type DictKey = keyof typeof zh;
