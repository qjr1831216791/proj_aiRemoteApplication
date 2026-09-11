# 调研报告：访问通道安全重构 —— 私有组网替代 frp 的方案选型

> - **日期**：2026-09-10
> - **状态**：已完成（结论待需求方决策，决策后走 spec 007 立项）
> - **范围**：需求方对 frp（SakuraFrp 穿透通道）安全性的质疑 → 现状威胁模型实证 + 替代方案四路取证（私有组网 / Tailscale / VPS 中枢 / Caddy mTLS）；连带覆盖 DDNS 直连通道的同类暴露面
> - **用途**：为后续 Spec 立项提供事实依据；先回答「现状到底有什么风险」，再回答「什么方案能满足『只要不泄露密钥就几乎不可能被入侵』且不丢现有功能」

---

## 1. 需求画像与安全目标

**需求方原话**（2026-09-10）：「我对于 frp 的安全性非常质疑……最好能做到只要我不泄露密钥就几乎不可能被入侵，需要满足当前的所有功能需求。」

**必须保留的功能需求**（自 spec 004/005/006 提取）：

| # | 需求 | 来源 |
|---|------|------|
| F1 | 浏览器访问 `https://ai.jackqi.cn`（标准 443），安卓 PWA 可用；访问端 = 用户自己的 Windows PC + 安卓手机（均在中国大陆） | 004 AC1、sprint0 试用记录 |
| F2 | 宿主机处于移动蜂窝热点（CGNAT，入站不可达）时远程访问仍可用 | 004 §1 |
| F3 | 直连（DDNS）通道保留为默认与回滚路径，与远程通道可切换、互斥运行 | 004 §2 产品约束 |
| F4 | 通道组件纳入工作台托管（自启/守护/收摊，无头可拉起） | 004 AC3/AC9/AC10 |
| F5 | 未来新机装机由向导代劳（006 穿透分支需可编排） | 006 US6 |
| F6 | 证书 DNS-01 自动续期不受通道形态影响 | 004 §4、006 AC7 |

**安全目标的形式化**：攻击者**不持有任何密钥/凭证**，且允许其：已知域名、全网段扫描（Censys/Shodan 级）、读取 CT 日志、被动监听链路——在此前提下：

1. **不可触达应用**（连 TLS 握手都完成不了，更谈不上利用应用漏洞）；
2. **不可解密流量**；
3. 尽量：不可实施有效拒绝服务；密钥万一泄露时，**损害可收敛**（可吊销/轮换，而非全网沦陷）。

---

## 2. 核心结论（先看这里）

1. **质疑成立，且问题比 frp 更大**：域名 `ai.jackqi.cn` 因 Let's Encrypt 强制进 CT 日志 + 全网段扫描，**必须视为全网可知**；后端应用 CloudCLI **有未认证 RCE 前科**（CVE-2026-31975，CVSS 9.8，利用代码公开）；Caddy 层零认证（本机 Caddyfile 实证）——**谁能触达 443，谁就曾有一条交付宿主机 shell 的公开路径**。此结论对 frp 通道与 DDNS 直连通道**同等成立**，直连通道甚至更裸（无任何中间方）。
2. **frp 的真实风险不在协议**：frp 协议 CVE 均在 frps 侧（服务商的机器），frpc 只出站不监听。真实风险是：①SakuraFrp access key 是**账户级凭据**，泄露 = 任意机器用同 `key:隧道ID` 顶号抢挂隧道、把你的域名流量导到攻击者机器；②key 明文在 frpc 命令行（Windows 任意本地用户经 WMI 可读）；③第三方节点可见元数据（SNI/访客 IP/流量特征）。
3. **「不泄露密钥 → 几乎不可入侵」的正确实现 = 把认证从应用层下沉到网络层或 TLS 握手层**：要么私有组网（未持密钥者的数据包在网络层就不可路由——零公网暴露），要么 Caddy mTLS（无客户端证书者 TLS 握手即被拒，扫描器拿不到任何 HTTP 指纹）。应用层认证（basic_auth 等）都达不到这个模型。
4. **主推荐（组合拳）**：
   - **穿透通道**：frp → **EasyTier（secure-mode）私有组网**，零成本起步（官方国内公共节点做握手/中继兜底）；可靠性不足再升级「腾讯云轻量大陆 VPS 做自有中枢」（约 ¥99/年促销价，公网只暴露一个静默丢包的 WireGuard/EasyTier UDP 端口）。
   - **直连通道**：保留 + **Caddy mTLS 客户端证书**收口（这是直连通道唯一的「不持证不可达」手段；组网方案不改变直连的公网属性）。
   - **过渡期 frp 加固三件套**（若保留 frp 一段时间）：key 移出命令行、frpc 开启访问认证、CloudCLI 升级纪律。
5. **Tailscale 列为备选**：免费条款最优（6 用户/设备不限量/关密钥过期免费），但本网络环境三重不利——中国控制面「能连但不可赌」、DERP 全境外、**移动对 UDP 有针对性 QoS**（多源实测：UDP 隧道丢包 20% 级）——双端 CGNAT 场景可靠性大概率**不如现有 frp 香港节点**；引入国内公网中继（Peer Relay）可改善，但那又回到「需要一台 VPS」。
6. **全部候选对既有 Spec 的影响可控**（§7）：004 通道语义反而简化（远程通道的 DNS 目标从 CNAME→节点域名 变为 A→静态虚拟 IP，一次配置不再切换）；006 穿透分支变简单（无实名、无建隧道，输入网络名+密钥即可）；005 心跳宿主侧探测照常工作。

---

## 3. 现状威胁模型（实证）

### 3.1 「隐匿」假设不成立——域名必然公开

- Let's Encrypt 证书**强制**记录进公共 CT 日志（Chrome/Safari 2018 起拒收无 SCT 证书），`crt.sh` 即可枚举全部子域名，是渗透侦察标准第一步（[Cloudflare CT 说明](https://www.cloudflare.com/learning/ssl/why-use-certificate-transparency)、[HackerTarget 实操](https://hackertarget.com/using-crtsh-certificate-transparency-recon-python)）。
- DDNS 的 A 记录本就在公共 DNS + 全网段扫描器（Censys/Shodan 扫 443）面前，从未宣传过的域名也会被扫到。
- frp 通道：共享节点按 SNI 分流，**SNI 就是路由键**——第三方对节点 IP:443 发 `SNI=ai.jackqi.cn` 即可触达本机 Caddy，与正常访客无异。
- **结论：必须假设两条通道的 443 均可被全网触达。** 防御界共识即放弃 security through obscurity。

### 3.2 后端应用：触达 = 拿 shell 的历史实证

- **CVE-2026-31975 / GHSA-gv8f-wpm2-m5wr**（Critical 9.8，2026-03-10 披露）：默认 JWT secret + WebSocket 认证绕过 → 经 `/shell` 端点**未认证 RCE**；影响 ≤1.24.0，1.25.0 修复；利用代码公开（[NVD](https://nvd.nist.gov/vuln/detail/CVE-2026-31975)、[GHSA](https://github.com/advisories/GHSA-gv8f-wpm2-m5wr)、[RAXE-2026-032](https://raxe.ai/labs/advisories/RAXE-2026-032)——后者并报告同包三重命令注入）。
- 本机部署 `@cloudcli-ai/cloudcli@1.37.3`（2026-09）**已覆盖 1.25.0 修复边界**，但该应用自带 shell 终端这一产品事实意味着：**任何新的同类漏洞，利用成功 = 宿主机 shell**。防护必须在「触达」之前，不能指望应用无漏洞。
- Caddy 层实证（本机 `Caddyfile`）：`ai.jackqi.cn:443 { tls { dns tencentcloud … } reverse_proxy 127.0.0.1:3001 }`——**除 TLS 加密外无任何认证**。

### 3.3 frp 通道专项风险

| 风险点 | 定性 | 事实 |
|--------|------|------|
| frp 协议 CVE | 服务商侧风险 | 均在 frps：CVE-2026-40910（vhost 认证绕过，Medium）、CVE-2026-73564（SSH 网关未认证打崩 frps，High，v0.53–0.70）；frpc 只出站不监听，远程攻击者无法直触（[frp advisories](https://github.com/fatedier/frp/security/advisories)） |
| access key 泄露 | **账户级沦陷** | key 即 `NATFRP_TOKEN`，面板/启动器 API 均可用其登录；攻击者可任意机器同 `key:隧道ID` 抢挂隧道 → 域名流量导到攻击者机器（钓鱼窗口期）、读改隧道配置、消耗流量；无法替你签证书（无 DNS 权），故劫持会被浏览器证书告警识破（[natfrp frpc 手册](https://doc.natfrp.com/frpc/manual.html)、[账户 FAQ](https://doc.natfrp.com/faq/account.html)） |
| 节点运营方 | 元数据级 | HTTPS 隧道 TLS 透传模式（本项目实际形态）下，节点可见 ClientHello 明文（SNI/访客 IP/流量特征）与密文；**不能静默 MITM**（无受信证书，替换证书当场被识破）；可掐断隧道 = DoS 能力 |
| 本机命令行暴露 | 真实暴露面 | Windows 任意本地用户经 `Get-CimInstance Win32_Process` 可读他用户进程完整命令行 → `frpc -f <key>:<id>` 的 key 对本机所有交互用户可见（004 plan §4.2 已知项，本次取证坐实） |
| 可用性 | 已实证 | 所在网络 DPI 拦截 frp 协议（004 §5 定案），且该网络对 UDP 普遍 QoS（§4.2） |

### 3.4 攻击链条总结

```
扫描/CT 发现域名 → 触达 443（两通道均可）→ TLS 层无认证门 → 直达 CloudCLI
→（应用漏洞窗口期，已有 RCE 前科且利用公开）→ 宿主机 shell
```

现状唯一的安全门是 TLS 加密，而 TLS 只加密不认证访客。**这就是需求方不适感的根源，且它不随「换掉 frp」自动消失**——DDNS 直连通道暴露面相同。

---

## 4. 候选方案取证

### 4.1 EasyTier（Rust，LGPL-3.0）——主推荐

> **两处事实修正（2026-09-10，spec 007 plan 取证）**：①许可证为 **LGPL-3.0**（2025-06 PR #951 起，非本报告初稿所写 Apache-2.0——官网页脚信息滞后；本项目以独立进程调用不链接，无传染义务）；②**官方不提供维护的公共服务器**（[隐私政策](https://easytier.cn/guide/privacy.html)明示仅发布软件；官方 Web 托管亦因合规收缩），原默认公共节点 `public.easytier.cn` 已 NXDOMAIN（本机 + 223.5.5.5 双源实测）——实际依赖的是社区公益节点（无 SLA）。

**架构**：去中心化组网，无中心账号体系；无公网 IP 时靠「共享节点」做握手与中继兜底。社区自建中转节点活跃（[官方 Discussion #2429](https://github.com/orgs/EasyTier/discussions/2429)，2026-08 仍有新增公益节点）。官方口径 UDP+TCP 双通道对称 NAT 打洞率约 98%（厂商数据，存疑）。

**安全模型**（[secure-mode 官方文档](https://easytier.cn/en/guide/network/secure-mode.html)，2026-09 更新）：

- 身份 = 网络名 + `network_secret` 共享密钥，WireGuard/AES-GCM 加密；
- **遗留模式**（默认注意关闭）：密钥泄露 = 全网流量可解，唯一补救是全网改密——**不可接受，必须启用 secure-mode**；
- **secure-mode**：Noise 握手 + 会话密钥轮换（前向保密）+ 共享节点公钥固定（防 MITM）+ **可签发带 TTL、可吊销的临时凭证**（访客设备发短时凭证而非主密码）——与「泄露可收敛」目标精确对齐。

**工程契合度**：

- **Android 官方 App**：APK 随 GitHub Releases 同发（v2.6.4，Android 7.0+），另有鸿蒙版（[下载页](https://easytier.cn/en/guide/download.html)）——F1 满足；
- **Windows 无头**：单 exe `easytier-core.exe`，官方支持「安装为 Windows 服务」与 `--no-tun` 模式（[服务文档](https://easytier.cn/guide/network/install-as-a-windows-service.html)）；可被工作台直接 spawn 托管（F4），亦可系统服务；
- 配置走 TOML 配置文件，`network_secret` 可落配置文件而非命令行（吸取 frp 命令行教训；具体字段 plan 阶段核验），栈目录 ACL 已由 `Protect-StackDir` 收紧；
- **零成本、零账号**：起步只用社区公益节点即可（F2：出站连接，CGNAT 无碍；多对端可配 + 自建 VPS 升级路径对冲无 SLA 风险）。

**风险**：社区公共节点中继的稳定性无 SLA；项目无外部安全审计、未检索到 CVE（存疑：可能只是没人查）；移动 UDP QoS（§4.2 同样适用）；自组网虚拟 IP 需规划私网段（默认 10.126.126.0/24）避免与局域网冲突。

### 4.2 Tailscale——备选（免费条款最优，本网络环境可靠性存疑）

**优势**（多源实证）：免费版 6 用户/用户设备不限量；**单台设备关闭 180 天密钥过期免费**（无人值守宿主机登录一次即可，[官方文档](https://tailscale.com/docs/features/access-control/key-expiry)）；auth key 免费可用（90 天上限，仅影响新机装机）；公共 DNS A 记录指向 100.x tailnet IP 有社区先例且 DNS-01 签发不受影响（[garrido.io 实操](https://garrido.io/notes/tailscale-nextdns-custom-domains/)）；Windows 客户端原生服务化、登录前常驻（[官方](https://tailscale.com/docs/set-up-servers/run-unattended)）；信任模型清晰（协调服务器只见公钥与元数据，中继只见密文；协调面宕机已建连接不死）。

**劣势（本项目网络环境）**：

- 中国控制面「能连但不可赌」：无全国性封锁实证，但省级不一致、社区教程普遍教走代理（[2025 实录](https://blog.l3zc.com/en/2025/04/tailscale-setup-recap/)）；
- DERP 中继**全境外无大陆节点**；双端 CGNAT 打洞常失败，回落中继 10ms→200ms 级（[实测](https://www.cnblogs.com/pDJJq/p/18982275/solve-tailscale-holes-derp-upnp-full-cone-nat-fwa5m)）；
- **移动对 UDP 有针对性 QoS**（多源一致）：联通↔移动 UDP 3Mbps 起丢包、UDP 隧道丢包 20% 级（[V2EX 实测一](https://www.v2ex.com/t/1222087)、[二](https://www.v2ex.com/t/1228109)）——本项目实测「所在网络拦 frp」即同一 UDP 环境；
- 2025-10 官方 Peer Relay 可用自己 tailnet 内公网设备做中继改善以上全部，但**前提仍是有一台国内公网设备**（= VPS）。

### 4.3 腾讯云轻量 VPS 自有中枢——可靠性升级位（按需）

**价格**（[官方价格总览](https://cloud.tencent.com/document/product/1207/73452)，2026-09 检索，需以购买页为准）：大陆入门 2C2G 4Mbps/300GB·¥48/月，**年付 85 折**；新用户促销档约 ¥99/年（可同价续费，[活动页](https://cloud.tencent.com/act/pro/lhsale)）；「锐驰型」2C2G 200Mbps 无限流量 ¥45~50/月（大文件天花板解除）。**香港档官方明示无法保障内地↔香港跨境质量、移动晚高峰常绕美日**（同页 + [第三方实测](https://www.hostol.com/archives/1586)）——**选大陆档**（两端都是移动网络，大陆内路径更可控）。

**要点**：

- WireGuard 部署教程充分（[腾讯云官方社区实例](https://cloud.tencent.com/developer/article/2438116)、[wg-easy 方案](https://cloud.tencent.com/developer/article/2644647)）；防火墙放行自定义 UDP 即时生效（[官方](https://cloud.tencent.com/document/product/1207/44577)）；
- **公网侧可只暴露一个 WireGuard UDP 端口**：WireGuard 对未认证包静默不响应（扫描不可见、协议不可探测，[官方设计](https://www.wireguard.com/quickstart/)）；管理走 OrcaTerm 免密登录**无需开 22**（[官方](https://cloud.tencent.com/document/product/1207/44642)）；
- **备案无涉**：备案针对「大陆服务器 + 域名对外开办网站/APP（HTTP）」；VPS 仅监听 UDP、无对外 HTTP、域名 A 记录只指私网/CGNAT 段，均不落入备案拦截范畴（[腾讯云备案规则](https://cloud.tencent.com/document/product/243/39038)，技术口径非法律意见）；
- 带宽适配：文本 AI 会话峰值需求远低于 1Mbps，4~5Mbps 足够；大文件 ≈ min(两端上行, 4Mbps) ≈ 0.5MB/s，月包 300GB 封顶后 ¥0.8/GB；
- 密钥模型：单 peer 泄露仅冒用该设备身份，删旧加新即吊销；中枢私钥泄露最重（需全网重配，属低概率运维事故）。

**同一台 VPS 四种用法的安全态势对比**：

| 用法 | 公网暴露面 | 密钥泄露后果 | 判定 |
|------|-----------|--------------|------|
| (a) WireGuard 中枢 | 1 个静默 UDP 端口 | 单 peer 泄露=冒用该设备，可吊销 | **最优**，与目标精确对齐 |
| (b) frps 自建 | frps 端口 + 对外 HTTP(S) 端口 | token 泄露可注册隧道；流量过服务器 | 大陆 IP + 域名对外 HTTP **触发备案**；移动网络拦 frp 已有前科 |
| (c) Headscale + 自建 DERP | 协调 API + DERP 443 + UDP | 控制面私钥泄露 = 全网沦陷 | 三组件运维最重；留给设备数增长后 |
| (d) nginx 443 反代对外 | 最大：全网可探测 | 无共享密钥 | 与零公网目标背道而驰；且触发备案 |

注：EasyTier 亦可把自有节点跑在该 VPS 上作为固定中继（替代公共节点），与 (a) 二选一或并存。

### 4.4 Caddy mTLS 客户端证书——两通道通用的纵深加固

**机制**：把认证下沉到 TLS 握手——无有效客户端证书者在握手期即被拒（TLS alert），**扫描器拿不到任何 HTTP 指纹**，应用漏洞对未持证者不可达。这是「不持证即不可达」在保留公网入口前提下的最强形态，也是**直连通道唯一的收口手段**。

**现行语法**（[Caddy 2.10 官方](https://caddyserver.com/docs/caddyfile/directives/tls)）：`tls { client_auth { mode require_and_verify; trust_pool file /path/ca.pem } }`（`trusted_ca_cert_file` 为旧写法；给了 trust_pool 时 mode 默认即 require_and_verify）。

**吊销/轮换**：Caddyfile 层暂无成熟 CRL 适配（[issue #7052](https://github.com/caddyserver/caddy/issues/7052)）；务实做法 = **客户端证书发短效期（30~90 天，过期即天然吊销）+ 轮换期 trust_pool 新旧 CA 并存**，代价约等于零停机。

**客户端支持矩阵**（多源实证）：Android Chrome ✓（系统设置装证书，弹框选择正常）；**iOS 仅 Safari**（Chrome on iOS 读不到钥匙串，[实测](https://www.reddit.com/r/selfhosted/comments/1k5t3fg/making_mtls_work_with_chrome_on_ios/)、[Pinterest 工程博客](https://medium.com/pinterest-engineering/employee-facing-mutual-tls-8643fe0cc0f9)）——本项目访问端为安卓，无碍；**PWA 独立窗口无系统性失效报告**，但有 Chromium 版本弹窗 bug 先例，**需真机 PWA 实测一次作为验收项**。

**与替代认证的对比**：basic_auth / forward_auth（Authelia 类）/ IP 白名单均非零信任——口令可爆破钓鱼、forward_auth 引入新攻击面、白名单对动态出口 IP 不现实。mTLS 是其中唯一「无凭证则连接不存在」的模型。

### 4.5 落选项速览

| 方案 | 落选原因 |
|------|----------|
| ZeroTier | 官方 root 全海外，国内直连失败率高、社区普遍需自建 Moon/私有 Planet（= 又要 VPS）；免费档 2024-08 砍至 10 设备（[HN](https://news.ycombinator.com/item?id=41127757)） |
| NetBird | 托管版 relay 无中国区（[issue #2950](https://github.com/netbirdio/netbird/issues/2950)）；自托管必须 VPS 自建 relay——可用性两头不占 |
| vnt | 国产 Rust 活跃、有官方 Android App，但**无凭证吊销机制**（密码泄露=全网沦陷）、Windows 无官方服务化——输给 EasyTier 的 secure-mode |
| 裸 WireGuard（无中继） | WireGuard 无内置打洞；双 CGNAT 且宿主机端点随时变，手机无从得知——不可行，除非自建 rendezvous（= 重造 EasyTier） |
| Cloudflare Tunnel | 域名需迁 NS；2026-09 调研国内速度实测不达标（004 §2 已定案） |

---

## 5. 横评总表

| 维度 | 现状 frp | 现状 + mTLS | EasyTier 公共节点 | EasyTier + VPS 节点 | Tailscale | Tailscale + Peer Relay(VPS) | VPS + WireGuard |
|------|---------|-------------|-------------------|---------------------|-----------|------------------------------|-----------------|
| 公网暴露面 | 节点 SNI 可达本机 443 | 同左，但握手即拒无证者 | **零**（纯出站） | **零**（VPS 仅静默 UDP 口） | **零** | **零** | **零**（VPS 仅静默 UDP 口） |
| 无密钥者可达性 | 可触达应用 | 仅到 TLS 拒绝 | 不可路由 | 不可路由 | 不可路由 | 不可路由 | 不可路由 |
| 密钥泄露后果 | 账户级：顶号抢挂/钓鱼窗口 | 同左+证书另有一层 | 遗留模式全网沦陷；**secure-mode 可吊销** | 同左 | 冒充单设备，可吊销 | 同左 | 单 peer 可吊销 |
| 中国可靠性 | 中（DPI 拦截已实证） | 同左 | 中（国内公共节点+移动 QoS 风险） | **高**（自有大陆中继） | 低-中（控制面+境外 DERP+UDP QoS） | 中-高 | **高**（自有大陆中继） |
| 安卓客户端 | 浏览器直用 | 浏览器+装客户端证书 | 官方 App | 官方 App | 官方 App | 官方 App | 官方/第三方 App |
| Windows 无头托管 | frpc 单 exe（现状） | 不变 | 单 exe+官方服务化 | 同左+VPS | 原生服务（但依赖其控制面） | 同左 | wg 客户端+VPS |
| 改动代价（004 语义） | — | 小（Caddyfile+证书脚本） | 中（换组件+设置项） | 中+购机 | 中 | 中+购机 | 中+购机 |
| 成本/年 | ¥0（流量签到制） | ¥0 | ¥0 | ~¥99（促销价） | ¥0 | ¥0+VPS | ~¥99 |

---

## 6. 推荐路径与待决策问题

### 6.1 推荐路径（分阶段，每步独立可回退）

- **P0 过渡加固（不动架构，半天量级，可先做）**
  1. frpc key 移出命令行（环境变量/配置文件注入，消灭 WMI 暴露面）；
  2. CloudCLI 纳入版本跟踪（当前 1.37.3 已过 CVE-2026-31975 修复线；升级前查 GHSA）+ Claude Code 本体同步更新（CVE-2026-35020 前科）；
  3. SakuraFrp 面板开启 frpc 访问认证（挡爆破纵深，[官方指南](https://doc.natfrp.com/bestpractice/frpc-auth.html)）。
- **P1 远程通道换 EasyTier（核心步，建议立项 spec 007）**：宿主机 easytier-core（secure-mode）+ 手机官方 App；域名 A 记录 → 虚拟 IP（静态，一次配置）；Caddy/证书/DNS-01 零改动；工作台托管复用 004 守护语义；直连⇄组网通道切换沿用互斥框架。真机实测中国可靠性（重点：移动热点↔手机蜂窝打洞/中继时延与稳定性），**实测不达标 → 升级 P2**。
- **P2（按需）腾讯云轻量大陆 VPS 做自有中枢**：EasyTier 自有节点或 WireGuard hub 二选一；公网仅静默 UDP 口 + OrcaTerm 管理；解除公共节点与移动 QoS 的可用性依赖。
- **P3 Caddy mTLS（纵深，直连通道唯一收口）**：自建 CA + 短效客户端证书脚本（沿 003/004 交互式脚本模式）；直连通道无证者握手即拒；组网通道可经 `verify_if_given` + 来源区分实现「mesh 内免证书、公网入口强制」（plan 阶段设计）；**PWA + 客户端证书真机实测**为验收前提。
- **frp 遗留处置（待决策）**：P1 稳定后退役 frpc（保留直连作回滚）；或保留为第三通道（代价：设置页/向导/守护三处复杂度常驻）。

### 6.2 待决策问题（需求方拍板后立项）

| # | 问题 | 建议 |
|---|------|------|
| Q1 | 远程通道换型是否立项（spec 007）？主选 EasyTier 还是直接带 VPS？ | 先 EasyTier 零成本实测，不达标再加 VPS |
| Q2 | frp 退役还是保留为第三通道？ | 建议 P1 稳定后退役，复杂度不常驻 |
| Q3 | mTLS 是否本期做？ | 建议做——它是直连通道唯一收口；PWA 真机实测先行验证 |
| Q4 | 若买 VPS：大陆档 + 仅 UDP + OrcaTerm 管理的形态是否接受？ | 是；先促销档 ¥99/年试一年 |
| Q5 | 005 心跳外部探测口径：封闭组网下外部 check-host 探测必然报「死」 | 建议维持本机视角口径（宿主是成员，经虚拟 IP 探测有效），放弃外部探测项 |
| Q6 | EasyTier 采遗留模式还是 secure-mode？ | **必须 secure-mode**（遗留模式密钥泄露=全网可解，与目标冲突） |

---

## 7. 对既有 Spec 的影响（供立项评估，不在本报告实施）

- **004-tunnel-access**（done）：远程通道组件替换（frpc → easytier-core）；Settings 的 `TunnelConfig` 字段语义变更（tunnel_id/node_domain → 网络名/凭证引用）；**DNS 切换语义简化**——组网 IP 静态，切通道= A 记录改指虚拟 IP + 暂停 ddns-go，不再有 CNAME 生命周期；守护/自启/收摊语义原样复用；AC1/3/8 等验收口径可映射沿用。落地时按变更流程回改 004 或在 007 中显式声明取代关系。
- **005-domain-heartbeat**（in-progress）：宿主机自身是组网成员，经域名（→虚拟 IP）的本机视角探测**照常有效**；外部 check-host 类探测在零公网架构下失去意义（对非成员永远「死」），建议维持本机视角口径（Q5）。
- **006-foolproof-install**（in-progress）：穿透分支反而简化——无实名、无外部建隧道，向导仅需「输入网络名 + 密钥（经脚本直写配置文件）+ 启动校验」；SakuraFrp 相关文案（zh/en）与 DNS CNAME 指引需随通道形态回修。006 尚在实施中，若 007 立项应先收口 006 再动通道层，避免向导返工两次。
- **宪法/ADR**：通道形态属局部选型，未达「全局级」；若最终走「VPS 中枢」则建议补一条 ADR（引入自建基础设施）。

---

## 8. 参考资料

**漏洞与暴露面**：[CVE-2026-31975 (NVD)](https://nvd.nist.gov/vuln/detail/CVE-2026-31975) · [GHSA-gv8f-wpm2-m5wr](https://github.com/advisories/GHSA-gv8f-wpm2-m5wr) · [RAXE-2026-032](https://raxe.ai/labs/advisories/RAXE-2026-032) · [frp security advisories](https://github.com/fatedier/frp/security/advisories) · [CT 日志原理（Cloudflare）](https://www.cloudflare.com/learning/ssl/why-use-certificate-transparency) · [crt.sh 侦察实操](https://hackertarget.com/using-crtsh-certificate-transparency-recon-python)

**SakuraFrp/frp**：[frpc 手册](https://doc.natfrp.com/frpc/manual.html) · [账户 FAQ](https://doc.natfrp.com/faq/account.html) · [官方安全指南](https://doc.natfrp.com/bestpractice/security.html) · [frpc 访问认证](https://doc.natfrp.com/bestpractice/frpc-auth.html) · [gofrp 文档](https://gofrp.org/zh-cn/docs/)

**EasyTier**：[官网下载](https://easytier.cn/en/guide/download.html) · [secure-mode 文档](https://easytier.cn/en/guide/network/secure-mode.html) · [Windows 服务文档](https://easytier.cn/guide/network/install-as-a-windows-service.html) · [GitHub Releases](https://github.com/EasyTier/EasyTier/releases) · [共享节点讨论 #2429](https://github.com/orgs/EasyTier/discussions/2429)

**Tailscale**：[定价](https://tailscale.com/pricing) · [key expiry](https://tailscale.com/docs/features/access-control/key-expiry) · [auth keys](https://tailscale.com/docs/features/access-control/auth-keys) · [无人值守](https://tailscale.com/docs/set-up-servers/run-unattended) · [安全页](https://tailscale.com/security) · [NAT 打洞原理](https://tailscale.com/blog/how-nat-traversal-works) · [100.x 自定义域名先例](https://garrido.io/notes/tailscale-nextdns-custom-domains/) · [中国可用性实录 2025](https://blog.l3zc.com/en/2025/04/tailscale-setup-recap/) · [移动 UDP QoS 实测（V2EX）](https://www.v2ex.com/t/1222087)

**腾讯云轻量**：[价格总览](https://cloud.tencent.com/document/product/1207/73452) · [防火墙操作](https://cloud.tencent.com/document/product/1207/44577) · [OrcaTerm 免密登录](https://cloud.tencent.com/document/product/1207/44642) · [备案规则](https://cloud.tencent.com/document/product/243/39038) · [WireGuard 组网实例](https://cloud.tencent.com/developer/article/2438116) · [香港线路实测](https://www.hostol.com/archives/1586)

**Caddy mTLS**：[tls 指令文档](https://caddyserver.com/docs/caddyfile/directives/tls) · [client_auth revocation 模块](https://caddyserver.com/docs/modules/tls.client_auth.revocation) · [CRL 适配 issue #7052](https://github.com/caddyserver/caddy/issues/7052) · [smallstep 浏览器证书指南](https://smallstep.com/docs/tutorials/browser-certificate-setup-guide/) · [iOS Chrome 客户端证书限制实测](https://www.reddit.com/r/selfhosted/comments/1k5t3fg/making_mtls_work_with_chrome_on_ios/)

---

## 变更记录

| 日期 | 说明 |
|------|------|
| 2026-09-10 | 成稿：现状威胁模型实证（CloudCLI CVE-2026-31975 + CT 暴露面 + SakuraFrp key 账户级风险）+ 四路方案取证（EasyTier/Tailscale/腾讯云轻量/Caddy mTLS）+ 横评与分阶段推荐。调研经 4 个并行子课题完成，MiniMax 中文搜索配额中途耗尽改走 WebSearch，GitHub 正文抓取受网络策略拦截处已用官方站文档交叉验证 |
