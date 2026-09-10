# 0003-caddy-dns-plugin-tls

- 状态: accepted
- 日期: 2026-09-10
- 关联: specs/006-foolproof-install/spec.md · specs/006-foolproof-install/plan.md · docs/research/sprint0-cloudcli-lan-deploy.md §9.3

## 背景与问题

sprint0 期 HTTPS 栈选择 acme.sh（Git Bash 脚本生态）签发/续期 `ai.jackqi.cn` 证书，DNS-01 经腾讯云 CAM API 验证。装机实践（006 立项动机）证明 acme.sh 是新机部署**最大的手工孤岛**：安装依赖 Git Bash 环境（未装 Git 的机器多一个前置）、两条手工命令行（`--issue` + `--install-cert`，且插件名必须传全名 `dns_tencent`——踩坑实录 §9.5-①）、独立续期计划任务、证书文件路径管理、`reloadcmd` 挂接 Caddy reload。每个环节都是装机摩擦与故障点，与 006"傻瓜式装机"目标直接冲突。

2026-09-10 调研确认三项事实：
1. 官方 caddy-dns 组织维护 [tencentcloud 插件](https://github.com/caddy-dns/tencentcloud)（v0.4.3 / 2025-11 仍活跃，9 个 releases），凭证体系为腾讯云 CAM SecretId/SecretKey——与栈内 ddns-go、`.env`（`TENCENT_SECRET_ID/KEY`，004 起 dns_api.rs 消费）**完全一致**；
2. Caddyfile 支持 `{env.*}` 占位符，密钥可经进程环境注入、不落明文配置（宪法 §3 兼容）；
3. caddyserver.com 按需构建服务可产出 `os=windows&arch=amd64` 带插件二进制（实测 HTTP 200 正常流出）。

## 决策

新机装机的 Caddy 统一采用**带 tencentcloud 插件的按需构建二进制**；Caddyfile 以 `tls { dns tencentcloud {env.TENCENT_SECRET_ID} {env.TENCENT_SECRET_KEY} }` 声明 DNS-01，签发与续期由 Caddy 进程自治。**acme.sh 及其续期计划任务在新机装机路径中退役**。

## 理由

- 消灭最大手工孤岛：装机少一个交互环节 + 一个前置（Git Bash），续期随 Caddy 进程自治、无需独立计划任务与 reloadcmd 挂接。
- 凭证单源：与 ddns-go、dns_api.rs 共用 `.env` 同一对 CAM 密钥，不新增凭证体系。
- DNS-01 免入站 443：穿透通道（004）下同样可先取证，向导阶段次序解耦（004 已实证该思路）。
- 放弃的备选 B（脚本包装 acme.sh 保持现状栈）：零架构变更但链路更长——Git Bash 前置、两条命令的包装健壮性、续期任务维护，把 acme.sh 的复杂性从用户转移给了脚本作者而非消灭。

## 后果

- 收益：装机路径组件数不变（仍是 caddy+ddns-go 两个二进制），但证书相关手工步骤归零；证书存储回归 Caddy 标准目录管理。
- 代价 1：Caddy 二进制获取从官方 GitHub Release 变为按需构建服务（编译分钟级 + 实测下载 ~36KB/s，全程可能 10 分钟上下）——对策：装机脚本分步进度提示 + 幂等跳过 + `-CaddyZip` 本地兜底（plan §7）。
- 代价 2：插件维护依赖 caddy-dns 组织（Caddy 官方插件生态，当前活跃）；版本锁定 `tencentcloud@v0.4.3` 并登记 manifest，升级为显式动作。
- 迁移与兼容：**已部署旧机不强制迁移**——acme.sh 链路继续工作，装机脚本对既有 Caddyfile 幂等不覆盖（006 spec 非目标）；向导只面向新机。
- 回退路径：若插件或构建服务失效，回退 B 路线（脚本包装 acme.sh），只影响新机装机路径（plan §7）。

## 契约变更

- Caddyfile 模板（新机）：`${Domain}:443 { tls { dns tencentcloud { secret_id {env.TENCENT_SECRET_ID} secret_key {env.TENCENT_SECRET_KEY} } } reverse_proxy 127.0.0.1:${Port} }`，顶部保留 `auto_https disable_redirects`（80 端口 Hyper-V 排除惯例不变）。
  **注意：tencentcloud 插件必须用块内键值（secret_id/secret_key）写法**——位置参数形式 validate 报 `wrong argument count or unexpected line ending`（2026-09-10 插件试用实证，install-https 模板已固化块写法）。
- Caddy 二进制下载源：`https://caddyserver.com/api/download?os=windows&arch=amd64&p=github.com/caddy-dns/tencentcloud@v0.4.3`（构建参数登记 manifest.json）。
- caddy spawn（工作台托管）新增：启动前从栈 `.env` 读入 `TENCENT_SECRET_ID/KEY` 注入进程环境（`{env.*}` 消费；密钥不落 Caddyfile 明文）。
