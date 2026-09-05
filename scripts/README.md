# 生命周期脚本（scripts）

> 项目日常开发入口：dev / build / test / lint / deploy 等命令的落点。
> 技术栈确定后，本目录的脚本通常只是 `package.json` / `Makefile` / `pyproject.toml` 的薄封装，保持"一个动作一条命令"。

## 约定

- 命令名与动作一致（`dev.sh`、`build.sh`…），不带子参数的复杂逻辑一律下沉到工具或源码。
- 与一次性辅助工具的边界见 [tools/README.md](../tools/README.md)。
- 新增命令后同步登记到根目录 `CLAUDE.md` 的"常用命令"一节。
