# 009-mesh-subnet-guard · 手工验收清单（acceptance-manual）

> **状态：已验收**（2026-09-11）——AC1~AC11 全过：自动化覆盖项实测全绿（§1）、
> T9 真机执行+机检核验、T3/T6 的 GUI/App 呈现按需求方签收指令确认（判定逻辑均有自动化覆盖，§2 注记）。
> spec 已流转 done。验收人：JackQi（需求方）

## 0. 前置速查

- 工作台开发态：`cd apps/workbench && npm run tauri dev`
- 旧通道残留检查：`tasklist | findstr /i "frpc ddns"`、`schtasks /query /tn "ddns-go Sprint0 autostart"`、`findstr SAKURA_FRP_KEY D:\Software\cloudcli-https\.env`
- 随包卸载脚本：`D:\Software\cloudcli-https\bin\uninstall-legacy.ps1`（以安装副本实际路径为准）
- 发版校验：`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-check.ps1`

## 1. 自动化覆盖项（已实测，2026-09-11）

| AC | 覆盖方式 | 结论 |
|----|----------|------|
| AC1（重叠阻断，不落盘） | Rust 单测：`render_config_checked_with_nics` 注入重叠网卡列表 → Err 含冲突 IP；渲染被拒即不落盘、不派发服务动作 | ✅ |
| AC2（不重叠放行） | Rust 单测：注入固定不冲突列表 → Ok，既有渲染断言全过（196 passed 基线） | ✅ |
| AC3（fail-open 可观察） | Rust 单测：空网卡列表 → Ok；跳过路径 `log::warn!("网卡信息不可得，跳过网段冲突检测…")` | ✅ |
| AC4（向导不可绕过） | 结构性保证：向导与设置卡同走 `mesh_apply_config` → `prepare_stack` → `render_config_checked` 单点接线（代码 doc 注释锚定）；接线层单测覆盖 | ✅ |
| AC5/AC6/AC7（release-check 三态） | T8 自测实录：正例退出 0（4/4）；`-Version 9.9.9` 退出 1（逐项定位）；TEMP 副本版本不一致退出 1；全程只读；BOM/CRLF/Parser 全过 | ✅ |
| AC11（TOML 格式合规） | Rust 单测：生成文本 `toml::from_str` 反序列化，断言 network_identity/peers 字段与 `MeshConfig` 一致、占位符非空 | ✅ |

回归门：`cargo test` 196 passed / 0 failed / 3 ignored；`cargo check --tests` 告警 0；`npm run build` 绿；zh/en 键集 248/248 双向一致；敏感扫描零命中（成员配置占位符不读真实密钥）。

## 2. 真机项（已收口）

### T3（AC1/AC2 手工复核 · 网段阻断）

1. 开发态或安装态工作台 → 设置页组网设置卡
2. 把虚拟网段改成与本机局域网同段（如 `192.168.X.0/24`，X 为本机实际网段；虚拟 IP 同步改入该段）→ 点应用
   - 预期：toast 报「虚拟网段 … 与本机物理网卡网段重叠（192.168.X.Y），请更换虚拟网段」；重开设置仍是旧值
3. 改回正常网段（如 `10.126.126.0/24`）→ 应用
   - 预期：正常保存/应用，无误报

**签收（2026-09-11）**：GUI 呈现按需求方签收指令确认——阻断/放行/不落盘三态判定均为 `render_config_checked_with_nics` 单测锁定（§1 AC1/AC2），GUI 走同一命令入口 `mesh_apply_config`（AC4 结构性保证），toast 通道为既有保存错误通道。

### T6（AC9/AC10 手工复核 · 成员配置对照）

1. 组网卡 → 「成员入网配置」折叠区展开
   - 预期：TOML 文本——网络名为实值、`network_secret = "<你的组网密钥>"` 占位、每个对端一条 `[[peer]]`、字段旁注释标注 App 输入项；「复制」按钮可用
2. 手机 EasyTier App 打开新建网络界面，逐字段对照
   - 预期：App 内「网络名称 / 网络密码 / 节点」与注释一一对应；按清单填入（密码填自定密钥）可正常入网
   - 附带核对：注释未提"成员手动填 IP"与 DHCP 默认行为的口径（若 App 默认 DHCP 即无需填 IP，以实测为准）

**签收（2026-09-11）**：App 对照呈现按需求方签收指令确认——TOML 字段正确性由单测反序列化断言锁定（§1 AC11），折叠区展示/复制/占位与指引文案为代码交付面（MeshCard 复用既有折叠区与 CopyButton 组件）；真实密钥不读不收（007 AC8 延续，敏感扫描零命中）。

### T9（AC8 · 真机旧通道清理，需求方在场）

1. 前置自检：组网服务在线（工作台组网卡四态为在线/连接中）、`D:\Software\cloudcli-https\.env` 已有腾讯密钥
2. 以管理员 PowerShell 执行随包 `uninstall-legacy.ps1`
3. 执行台账摘要回填 specs/008 的 acceptance-manual（AC10/AC11 备注区）

**已执行（2026-09-11）**：核验通过——四类残留全清（进程无/自启任务已注销/四个退役文件已删/`SAKURA_FRP_KEY` 赋值行已移除且 TENCENT_* 完好）；附带确认 EasyTierMesh Running、3001/443 监听与 TUN 虚拟 IP 正常（现役零误伤）。详见 specs/008 acceptance-manual 签收依据后注。

## 3. 验收汇总（已回填，2026-09-11）

| AC | 结论 | 备注 |
|----|------|------|
| AC1~AC4 | ✅ | 自动化实测全绿；GUI 呈现按需求方签收指令确认（T3 注记） |
| AC5~AC7 | ✅ | T8 三态自测 |
| AC8 | ✅ | 2026-09-11 真机执行 + 机检核验（spec 008 同日回填） |
| AC9/AC10 | ✅ | AC11 单测锁定字段；App 对照按需求方签收指令确认（T6 注记） |
| AC11 | ✅ | 单测反序列化断言 |

**结论：AC1~AC11 全部通过，spec 009 于 2026-09-11 验收 done（需求方：JackQi）。**
