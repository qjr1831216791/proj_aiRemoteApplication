# 013-domain-single-source · 验收记录（acceptance-manual）

> 导航：[spec.md](./spec.md) · [plan.md](./plan.md) · [tasks.md](./tasks.md) · 返回 [MOC](../MOC.md)

- **创建日期**: 2026-09-25

## 验收结论

**✅ 2026-09-25 需求方真机签收，AC1~AC10 全部验证通过，状态 in-progress → done。**

- 验收背景：分发用户真机暴露两项域名硬编码缺陷（2026-09-15），实施期间同事真机复现第三项（同步 DNS 回落研发者域名，AC9/AC10 由此增设）——三项均已在验收期修复并合入（提交 205889e、a9adf39、e071adc 等，详见 CHANGELOG 0.8.0 Fixed 段）
- 验收范围：装机向导域名录入双写 → `settings.domain` 唯一写入口 → 同步 DNS / 地址区 / 心跳 / 托盘 / 诊断 / 只读卡 / i18n 文案 / 装机与卸载脚本参数化全链路消费生效域名
- 佐证：cargo 264~274 绿（各阶段）、前端构建绿、分发真机走查通过（需求方确认）
- 遗留说明：无
