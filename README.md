# AI 远程应用（proj_aiRemoteApplication）

> 一句话定位待补充，见 [docs/product.md](docs/product.md)。

早期阶段项目，采用 **Spec 驱动开发（Spec-Driven Development）**：需求先落于文档、实现可追溯、变更有历史。

## 文档地图

| 想了解 | 去哪 |
|--------|------|
| 开发规范与协作约定 | [CLAUDE.md](CLAUDE.md) |
| 需求与进行中的功能 | [specs/MOC.md](specs/MOC.md) |
| 产品愿景与路线图 | [docs/product.md](docs/product.md) |
| 不可违背的工程约束 | [docs/constitution.md](docs/constitution.md) |
| 技术决策及其缘由 | [docs/adr/](docs/adr/) |
| 版本说明与变更历史 | [CHANGELOG.md](CHANGELOG.md) |

## 快速开始

Sprint 0（v0.1.0）已交付脚本化试用基建：用现成的 CloudCLI 验证"手机遥控开发机上的 Claude Code"。开发机双击 [tools/sprint0/start-here.bat](tools/sprint0/start-here.bat) 进入总控菜单，上路引导见 [tools/sprint0/README.md](tools/sprint0/README.md)。

本地环境变量：`cp .env.example .env` 后填入真实值。

**桌面工作台**（Sprint 1 交付，v0.2.0，[specs/001-desktop-console](specs/001-desktop-console/spec.md)）：托盘常驻的图形控制台，接管上述脚本的总控职责——三组件状态与一键启停、自启托管、可控退出、低频操作入口，中英双语。**不想敲命令就双击 [apps/workbench/start-here.bat](apps/workbench/start-here.bat)**：数字菜单覆盖开发运行 / 测试 / 前端构建 / 一键打包等日常动作。手动等价命令：

```bash
cd apps/workbench
npm install
npm run tauri dev    # 开发态（自动带起前端与 cargo）；npm run build 仅构建前端
cargo test --manifest-path apps/workbench/src-tauri/Cargo.toml   # Rust 侧全套测试
```

一键打包分发（Windows PowerShell）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
```

产出双形态产物落 `release/`（已 .gitignore）：`AI-Remote-Workbench_<版本>_x64-setup.exe`（NSIS 安装器，离线可装）与 `AI-Remote-Workbench_<版本>_x64.zip`（便携版，解压即用）；sprint0 脚本以内置副本随包分发，覆盖安装升级保留设置。未签名分发可能触发 SmartScreen，选择"仍要运行"（引导见便携包内 README.txt）。

**远程访问方案**（v0.4.0 起，spec [007](specs/007-mesh-access/spec.md) / [008](specs/008-legacy-channel-removal/spec.md)；边界收口见 [010](specs/010-lan-boundary-hardening/spec.md)）：主方案为 **EasyTier 组网**——成员设备经 `https://ai.jackqi.cn` 访问（域名指向组网虚拟 IP，公网不可路由），443 侧以「源地址 ∈ 组网网段 + TUN 接口」白名单放行（ADR-0005），未持组网密钥的设备在网络层不可达，零公网暴露；**局域网 IP 直访默认已收口**（3001 默认拒绝，443 不再有按网络归类的放行），临时需要时经工作台例外开关开启（仅专用网络 + 本机子网，12h 自动回落）。旧穿透（frp/SakuraFrp）与直连（ddns-go）通道已退役，老装机残留可按工作台提示运行内置的 `uninstall-legacy.ps1` 清理。

## 目录结构

以 [CLAUDE.md](CLAUDE.md) 的"目录结构"一节为唯一来源，此处不重复维护。

## 许可证

待定。
