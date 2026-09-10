//! 腾讯云 DNS 记录自动切换（spec 004 AC12/13 语义升级：自动执行 + 手动兜底）。
//!
//! - TC3-HMAC-SHA256 签名调 `dns.tencentcloudapi.com`（2021-03-23）
//! - 凭证复用机器上已有的腾讯云密钥（ddns-go 正在用同一密钥写解析）：
//!   栈目录 `.env`（TENCENT_SECRET_ID/KEY）→ `ddns-go.yaml` dnsconf 段回退
//! - 纯函数（凭证解析 / TC3 签名 / 记录调和 reconcile）与 HTTP 采集隔离；
//!   TC3 正确性由真机调用验证（签名错则 API 报 AuthFailure）
//! - 同步失败由调用方降级：不阻断通道切换，检测循环继续显示手动指引

use crate::consts::{DOMAIN, DOMAIN_ROOT, FRPC_ENV_FILE, DEFAULT_STACK_DIR};
use crate::tunnel::parse_env_value;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// 腾讯云 API 密钥（SecretId/SecretKey；只内存传递，禁止日志）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcCredential {
    pub id: String,
    pub key: String,
}

// ── 凭证读取（回退链：.env → ddns-go.yaml）─────────────────────────────────

/// 读取腾讯云凭证（None = 两处都没有 → 调用方跳过自动切换走手动指引）
pub fn read_credential(stack_dir: &str) -> Option<TcCredential> {
    let env_path = std::path::PathBuf::from(stack_dir).join(FRPC_ENV_FILE);
    if let Ok(content) = std::fs::read_to_string(&env_path) {
        if let Some(cred) = parse_env_creds(&content) {
            return Some(cred);
        }
    }
    let yaml_path = std::path::PathBuf::from(stack_dir).join("ddns-go.yaml");
    if let Ok(content) = std::fs::read_to_string(&yaml_path) {
        if let Some(cred) = parse_yaml_creds(&content) {
            return Some(cred);
        }
    }
    None
}

/// 从 `.env` 内容提取 TENCENT_SECRET_ID/KEY
pub fn parse_env_creds(content: &str) -> Option<TcCredential> {
    let id = parse_env_value(content, "TENCENT_SECRET_ID")?;
    let key = parse_env_value(content, "TENCENT_SECRET_KEY")?;
    Some(TcCredential { id, key })
}

/// 从 ddns-go.yaml 内容提取 dnsconf 段的 id/secret（凭证已在机器上，
/// 复用避免让用户重复提供；按行匹配键名，值取首个冒号后内容）
pub fn parse_yaml_creds(content: &str) -> Option<TcCredential> {
    let mut id = None;
    let mut key = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if id.is_none() && trimmed.starts_with("id:") {
            id = Some(trimmed["id:".len()..].trim().to_string());
        } else if key.is_none() && trimmed.starts_with("secret:") {
            key = Some(trimmed["secret:".len()..].trim().to_string());
        }
    }
    // 防误匹配：空值/占位视为缺失
    let id = id.filter(|s| !s.is_empty())?;
    let key = key.filter(|s| !s.is_empty())?;
    Some(TcCredential { id, key })
}

// ── TC3-HMAC-SHA256 签名（纯函数）──────────────────────────────────────────

type HmacSha256 = Hmac<Sha256>;

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 任意长度密钥");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// SHA256 hex(公开:frpc 下载恢复的完整性校验复用)
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// 组装 Authorization 头（纯函数：全部输入显式，可单测）。
/// 形态与官方 SDK 对齐（2026-09-10 实测：通用 TC3 三头签名被网关拒
/// AuthFailure.SignatureFailure，官方实现只签 content-type;host 两头）。
pub fn build_auth_header(
    cred: &TcCredential,
    _action: &str,
    body: &str,
    ts: u64,
    date: &str,
) -> String {
    let content_type = "application/json";
    // CanonicalRequest：六段 = method\n uri\n query\n headers\n signed\n hash。
    // headers 段自带尾换行，与格式串的分隔换行叠加 → SignedHeaders 前有空行
    // （对拍官方 SDK abstract_client._get_tc3_signature 逐字节确认）
    let canonical = format!(
        "POST\n/\n\ncontent-type:{content_type}\nhost:{API_HOST}\n\ncontent-type;host\n{}",
        sha256_hex(body.as_bytes())
    );
    // StringToSign 四行结构：算法 \n 时间戳 \n 凭证范围(date/service/tc3_request) \n 哈希
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{ts}\n{date}/{API_SERVICE}/tc3_request\n{}",
        sha256_hex(canonical.as_bytes())
    );
    // 签名密钥链
    let k_date = hmac_sha256(format!("TC3{}", cred.key).as_bytes(), date.as_bytes());
    let k_service = hmac_sha256(&k_date, API_SERVICE.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"tc3_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));
    format!(
        "TC3-HMAC-SHA256 Credential={}/{}/{API_SERVICE}/tc3_request, SignedHeaders=content-type;host, Signature={signature}",
        cred.id, date
    )
}

// ── 记录调和（纯函数：当前记录 + 目标状态 → 操作集）─────────────────────────

/// 一条 DNS 记录（DescribeRecordList 输出的裁剪视图）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsRecord {
    pub record_id: u64,
    pub rtype: String,
    pub value: String,
    /// 解析记录是否启用（dnspod Status=ENABLE/DISABLE，即控制台的暂停开关）
    pub enabled: bool,
}

/// 调和操作（执行器按序应用）。**暂停/激活语义**（需求方 2026-09-10 提议，
/// 优于删除/重建：记录 ID 保留、完全可逆、不产生重建垃圾）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOp {
    /// 启用/暂停一条既有记录（ModifyRecordStatus）
    SetStatus { record_id: u64, enable: bool },
    /// 新建记录（CreateRecord；仅目标记录不存在时）
    Create { rtype: String, value: String },
}

/// 调和：把 `sub` 记录集合推向目标状态（纯函数，幂等——已满足则空操作集）。
/// - `want_cname = Some(v)`（穿透）：全部 A 暂停；CNAME 值对 → 激活，
///   值错 → 暂停（保记录）并新建对的；缺失 → 新建
/// - `want_cname = None`（直连）：全部 CNAME 暂停；A → 激活
///   （A 记录值由 ddns-go 维护；不存在时也由 ddns-go 启动时自动新建）
pub fn reconcile(current: &[DnsRecord], want_cname: Option<&str>) -> Vec<RecordOp> {
    let mut ops = Vec::new();
    let norm = |s: &str| s.trim_end_matches('.').to_ascii_lowercase();
    let is = |r: &DnsRecord, t: &str| r.rtype.eq_ignore_ascii_case(t);
    match want_cname {
        Some(target) => {
            for r in current.iter().filter(|r| is(r, "A")) {
                if r.enabled {
                    ops.push(RecordOp::SetStatus { record_id: r.record_id, enable: false });
                }
            }
            let cnames: Vec<&DnsRecord> = current.iter().filter(|r| is(r, "CNAME")).collect();
            match cnames.iter().copied().find(|r| norm(&r.value) == norm(target)) {
                Some(r) => {
                    if !r.enabled {
                        ops.push(RecordOp::SetStatus { record_id: r.record_id, enable: true });
                    }
                    // 其余值不符的 CNAME 暂停（保记录可回溯）
                    for r in cnames.iter().filter(|r| norm(&r.value) != norm(target)) {
                        if r.enabled {
                            ops.push(RecordOp::SetStatus { record_id: r.record_id, enable: false });
                        }
                    }
                }
                None => {
                    for r in &cnames {
                        if r.enabled {
                            ops.push(RecordOp::SetStatus { record_id: r.record_id, enable: false });
                        }
                    }
                    ops.push(RecordOp::Create { rtype: "CNAME".into(), value: target.to_string() });
                }
            }
        }
        None => {
            for r in current.iter().filter(|r| is(r, "CNAME")) {
                if r.enabled {
                    ops.push(RecordOp::SetStatus { record_id: r.record_id, enable: false });
                }
            }
            for r in current.iter().filter(|r| is(r, "A")) {
                if !r.enabled {
                    ops.push(RecordOp::SetStatus { record_id: r.record_id, enable: true });
                }
            }
        }
    }
    ops
}

// ── HTTP 执行（ureq；调用方 spawn_blocking 包裹）───────────────────────────

/// DNSPod API（腾讯云 DNS 解析产品线：service=dnspod，与产品页语义一致）
const API_HOST: &str = "dnspod.tencentcloudapi.com";
const API_SERVICE: &str = "dnspod";
const API_VERSION: &str = "2021-03-23";

/// 单次 API 调用（POST + TC3 签名；Response.Error 存在即 Err）
fn call_api(cred: &TcCredential, action: &str, params: &serde_json::Value) -> Result<serde_json::Value, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    signed_request(cred, action, params, ts)
}

/// 带显式时间戳的签名请求（ts 注入便于调试重放；生产走 call_api）
pub fn signed_request(
    cred: &TcCredential,
    action: &str,
    params: &serde_json::Value,
    ts: u64,
) -> Result<serde_json::Value, String> {
    let body = serde_json::json!(params).to_string();
    let date = utc_date(ts);
    let auth = build_auth_header(cred, action, &body, ts, &date);

    let resp = ureq::post(&format!("https://{API_HOST}"))
        .timeout(std::time::Duration::from_secs(10))
        .set("Content-Type", "application/json")
        .set("X-TC-Action", action)
        .set("X-TC-Version", API_VERSION)
        .set("X-TC-Timestamp", &ts.to_string())
        .set("Authorization", &auth)
        .send_string(&body)
        .map_err(|e| format!("{action} 请求失败：{e}"))?;
    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("{action} 响应解析失败：{e}"))?;
    if let Some(err) = json.pointer("/Response/Error") {
        return Err(format!(
            "{action} 失败：{}（{}）",
            err.get("Message").and_then(|m| m.as_str()).unwrap_or("?"),
            err.get("Code").and_then(|c| c.as_str()).unwrap_or("?")
        ));
    }
    Ok(json)
}

/// epoch 秒 → UTC YYYY-MM-DD（无 chrono 依赖的轻量实现；2026±50 年内正确）
pub fn utc_date(ts: u64) -> String {
    let days = ts / 86_400;
    // Howard Hinnant 的 civil_from_days 算法（公历无脑换算）
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// 列出子域记录（example/诊断用公开封装；内部走 list_records）
pub fn list_records_probe(cred: &TcCredential, root: &str, sub: &str) -> Vec<DnsRecord> {
    list_records(cred, root, sub).unwrap_or_default()
}

/// 列出子域记录（DescribeRecordList；自动翻页上限 3 页防失控）
fn list_records(cred: &TcCredential, root: &str, sub: &str) -> Result<Vec<DnsRecord>, String> {
    let mut out = Vec::new();
    let mut offset = 0u64;
    for _page in 0..3 {
        let json = call_api(
            cred,
            "DescribeRecordList",
            &serde_json::json!({ "Domain": root, "SubDomain": sub, "Offset": offset, "Limit": 100 }),
        )?;
        let list = json
            .pointer("/Response/RecordList")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        for r in list {
            out.push(DnsRecord {
                record_id: r.get("RecordId").and_then(|v| v.as_u64()).unwrap_or(0),
                rtype: r.get("Type").and_then(|v| v.as_str()).unwrap_or("").into(),
                value: r.get("Value").and_then(|v| v.as_str()).unwrap_or("").into(),
                enabled: r.get("Status").and_then(|v| v.as_str()) == Some("ENABLE"),
            });
        }
        // 无更多数据（ListOver 标志或不足一页）即停
        let over = json.pointer("/Response/ListOver").and_then(|v| v.as_bool()).unwrap_or(true);
        let count = out.len() as u64;
        if over || count < offset + 100 {
            break;
        }
        offset += 100;
    }
    Ok(out.into_iter().filter(|r| r.record_id != 0).collect())
}

fn create_record(cred: &TcCredential, root: &str, sub: &str, rtype: &str, value: &str) -> Result<(), String> {
    call_api(
        cred,
        "CreateRecord",
        &serde_json::json!({ "Domain": root, "SubDomain": sub, "RecordType": rtype, "RecordLine": "默认", "Value": value }),
    )
    .map(|_| ())
}

fn set_record_status(cred: &TcCredential, root: &str, record_id: u64, enable: bool) -> Result<(), String> {
    call_api(
        cred,
        "ModifyRecordStatus",
        &serde_json::json!({ "Domain": root, "RecordId": record_id, "Status": if enable { "ENABLE" } else { "DISABLE" } }),
    )
    .map(|_| ())
}

// ── 高层切换（switch_channel 消费）─────────────────────────────────────────

/// 切到穿透：A 暂停 + CNAME 激活/新建指向节点域名（幂等）
pub fn sync_to_tunnel(cred: &TcCredential, root: &str, sub: &str, node_domain: &str) -> Result<usize, String> {
    let current = list_records(cred, root, sub)?;
    let mut applied = 0;
    for op in reconcile(&current, Some(node_domain)) {
        match op {
            RecordOp::SetStatus { record_id, enable } => set_record_status(cred, root, record_id, enable)?,
            RecordOp::Create { rtype, value } => create_record(cred, root, sub, &rtype, &value)?,
        }
        applied += 1;
    }
    Ok(applied)
}

/// 切回直连：CNAME 暂停 + A 激活（A 值由 ddns-go 维护/重建）
pub fn sync_to_direct(cred: &TcCredential, root: &str, sub: &str) -> Result<usize, String> {
    let current = list_records(cred, root, sub)?;
    let mut applied = 0;
    for op in reconcile(&current, None) {
        if let RecordOp::SetStatus { record_id, enable } = op {
            set_record_status(cred, root, record_id, enable)?;
            applied += 1;
        }
    }
    Ok(applied)
}

/// 域名常量派生子域（"ai.jackqi.cn" + "jackqi.cn" → "ai"）
pub fn subdomain_of<'a>(domain: &'a str, root: &str) -> &'a str {
    domain
        .strip_suffix(root)
        .map(|s| s.trim_end_matches('.'))
        .filter(|s| !s.is_empty())
        .unwrap_or(domain)
}

// ── 单元测试（纯函数；HTTP 不在覆盖范围）────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── spec 007 T2-③ bogon 解析实测（手动两段式，真实 API）──────────────────
    // 用法：
    //   ① cargo test bogon_probe_create -- --ignored --nocapture
    //   ② 外部验证公共递归是否照常返回私网段 A 值：
    //      nslookup meshprobe.jackqi.cn 223.5.5.5   （阿里）
    //      nslookup meshprobe.jackqi.cn 119.29.29.29（DNSPod）
    //      nslookup meshprobe.jackqi.cn 8.8.8.8     （Google）
    //      nslookup meshprobe.jackqi.cn             （本网络默认/运营商）
    //   ③ cargo test bogon_probe_cleanup -- --ignored --nocapture
    // 结论回填 specs/007-mesh-access/tasks.md 附注③。

    /// 建临时记录 meshprobe.jackqi.cn A 10.126.126.1（幂等：残留先删）
    #[test]
    #[ignore = "spec007 T2-③：真实 DNSPod API，建私网 A 记录供外部递归验证"]
    fn bogon_probe_create() {
        let cred = read_credential(crate::consts::DEFAULT_STACK_DIR).expect("栈目录凭证缺失");
        for r in list_records(&cred, crate::consts::DOMAIN_ROOT, "meshprobe").unwrap_or_default() {
            call_api(
                &cred,
                "DeleteRecord",
                &serde_json::json!({ "Domain": crate::consts::DOMAIN_ROOT, "RecordId": r.record_id }),
            )
            .expect("残留记录清理失败");
        }
        create_record(&cred, crate::consts::DOMAIN_ROOT, "meshprobe", "A", "10.126.126.1")
            .expect("建 A 记录失败");
        println!("已创建 meshprobe.{} A 10.126.126.1，请从各公共递归 nslookup 验证", crate::consts::DOMAIN_ROOT);
    }

    /// 删除 bogon 实测记录（验证完必跑，不留残留）
    #[test]
    #[ignore = "spec007 T2-③：清理 bogon 实测记录"]
    fn bogon_probe_cleanup() {
        let cred = read_credential(crate::consts::DEFAULT_STACK_DIR).expect("栈目录凭证缺失");
        let records = list_records(&cred, crate::consts::DOMAIN_ROOT, "meshprobe").unwrap_or_default();
        assert!(!records.is_empty(), "meshprobe 记录不存在（可能已清理）");
        for r in records {
            call_api(
                &cred,
                "DeleteRecord",
                &serde_json::json!({ "Domain": crate::consts::DOMAIN_ROOT, "RecordId": r.record_id }),
            )
            .expect("删除失败");
        }
        println!("meshprobe 记录已清理");
    }

    #[test]
    fn env_creds_parsed() {
        let env = "SAKURA_FRP_KEY=x\nTENCENT_SECRET_ID=AKIDtest\nTENCENT_SECRET_KEY=keytest\n";
        let cred = parse_env_creds(env).expect("应有凭证");
        assert_eq!(cred.id, "AKIDtest");
        assert_eq!(cred.key, "keytest");
        assert_eq!(parse_env_creds("TENCENT_SECRET_ID=only"), None, "缺任一即缺失");
    }

    #[test]
    fn yaml_creds_parsed_with_indent() {
        // 真实 ddns-go.yaml 形态：dnsconf.dns.id/secret（缩进 8 空格）
        let yaml = "dnsconf:\n    - name: \"\"\n      ipv4:\n        enable: true\n      dns:\n        name: tencentcloud\n        id: AKIDyaml\n        secret: secredns\n      ttl: \"\"\n";
        let cred = parse_yaml_creds(yaml).expect("应有凭证");
        assert_eq!(cred.id, "AKIDyaml");
        assert_eq!(cred.key, "secredns");
        assert_eq!(parse_yaml_creds("user:\n  username: jackqi"), None);
    }

    #[test]
    fn auth_header_shape() {
        let cred = TcCredential { id: "AKIDx".into(), key: "ky".into() };
        let h = build_auth_header(&cred, "CreateRecord", "{\"a\":1}", 1000, "2026-09-10");
        assert!(h.starts_with("TC3-HMAC-SHA256 Credential=AKIDx/2026-09-10/dnspod/tc3_request"), "{h}");
        // 与官方 SDK 对齐：只签 content-type;host（三头形态被网关拒绝，实测）
        assert!(h.contains("SignedHeaders=content-type;host,"));
        assert!(!h.contains("x-tc-action"));
        assert!(h.contains("Signature="));
    }

    #[test]
    fn reconcile_tunnel_pauses_a_and_activates_cname() {
        // 昨晚实测的混挂场景（A 活跃 + CNAME 暂停）→ 暂停 A、激活 CNAME
        let current = vec![
            DnsRecord { record_id: 1, rtype: "A".into(), value: "117.182.118.202".into(), enabled: true },
            DnsRecord { record_id: 2, rtype: "CNAME".into(), value: "frp-can.com.".into(), enabled: false },
        ];
        let ops = reconcile(&current, Some("frp-can.com"));
        assert_eq!(
            ops,
            vec![
                RecordOp::SetStatus { record_id: 1, enable: false },
                RecordOp::SetStatus { record_id: 2, enable: true },
            ]
        );

        // 无 CNAME → 建（A 暂停）；值错 → 暂停错的 + 建对的
        let no_cname = vec![DnsRecord { record_id: 3, rtype: "A".into(), value: "1.2.3.4".into(), enabled: false }];
        assert_eq!(
            reconcile(&no_cname, Some("frp-can.com")),
            vec![RecordOp::Create { rtype: "CNAME".into(), value: "frp-can.com".into() }],
            "A 已暂停、无 CNAME → 只建 CNAME"
        );
        let wrong = vec![DnsRecord { record_id: 4, rtype: "CNAME".into(), value: "other.com".into(), enabled: true }];
        assert_eq!(
            reconcile(&wrong, Some("frp-can.com")),
            vec![
                RecordOp::SetStatus { record_id: 4, enable: false },
                RecordOp::Create { rtype: "CNAME".into(), value: "frp-can.com".into() },
            ]
        );
        // 已对齐（A 已暂停 + CNAME 活跃值对）→ 幂等空操作
        let aligned = vec![
            DnsRecord { record_id: 5, rtype: "A".into(), value: "1.2.3.4".into(), enabled: false },
            DnsRecord { record_id: 6, rtype: "CNAME".into(), value: "frp-can.com".into(), enabled: true },
        ];
        assert!(reconcile(&aligned, Some("frp-can.com")).is_empty());
    }

    #[test]
    fn reconcile_direct_pauses_cname_and_activates_a() {
        // 切回直连：CNAME 暂停、A 激活（A 值由 ddns-go 维护，不动）
        let current = vec![
            DnsRecord { record_id: 5, rtype: "CNAME".into(), value: "frp-can.com".into(), enabled: true },
            DnsRecord { record_id: 6, rtype: "A".into(), value: "1.2.3.4".into(), enabled: false },
        ];
        assert_eq!(
            reconcile(&current, None),
            vec![
                RecordOp::SetStatus { record_id: 5, enable: false },
                RecordOp::SetStatus { record_id: 6, enable: true },
            ]
        );
        // 无 A 记录：交给 ddns-go 自动新建，调和层不代建
        let no_a = vec![DnsRecord { record_id: 7, rtype: "CNAME".into(), value: "frp-can.com".into(), enabled: true }];
        assert_eq!(reconcile(&no_a, None), vec![RecordOp::SetStatus { record_id: 7, enable: false }]);
    }

    #[test]
    fn subdomain_derivation() {
        assert_eq!(subdomain_of("ai.jackqi.cn", "jackqi.cn"), "ai");
        assert_eq!(subdomain_of("jackqi.cn", "jackqi.cn"), "jackqi.cn", "根域名兜底");
    }

    #[test]
    fn civil_date_conversion() {
        // 已知锚点：1970-01-01 epoch 0；2026-08-25 / 2026-09-10 00:00 UTC
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(1_787_616_000), "2026-08-25");
        assert_eq!(utc_date(1_788_998_400), "2026-09-10");
        assert_eq!(utc_date(1_788_998_399), "2026-09-09", "日界边界");
    }
}
