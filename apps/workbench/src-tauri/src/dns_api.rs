//! 腾讯云 DNS 记录调和（spec 008 收敛：组网单通道——A=虚拟 IP）。
//!
//! - TC3-HMAC-SHA256 签名调 `dns.tencentcloudapi.com`（2021-03-23）
//! - 凭证单源：栈目录 `.env`（TENCENT_SECRET_ID/KEY，set-tencent-key.ps1 写出；
//!   Caddy DNS-01 续期同一来源）。原 ddns-go.yaml 回退链随直连通道退役（D3）
//! - 纯函数（凭证解析 / TC3 签名 / 记录调和 reconcile / DNS 对齐判定）与
//!   HTTP 采集隔离；TC3 正确性由真机调用验证（签名错则 API 报 AuthFailure）
//! - 同步失败由调用方降级：不阻断操作，检测循环继续显示手动指引

use crate::consts::STACK_ENV_FILE;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// 腾讯云 API 密钥（SecretId/SecretKey；只内存传递，禁止日志）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcCredential {
    pub id: String,
    pub key: String,
}

// ── 凭证读取（唯一来源：栈目录 .env）────────────────────────────────────────

/// 读取腾讯云凭证（None = `.env` 未配置 → 调用方跳过自动切换走手动指引）
pub fn read_credential(stack_dir: &str) -> Option<TcCredential> {
    let env_path = std::path::PathBuf::from(stack_dir).join(STACK_ENV_FILE);
    let content = std::fs::read_to_string(&env_path).ok()?;
    parse_env_creds(&content)
}

/// 从 `.env` 内容提取 TENCENT_SECRET_ID/KEY
pub fn parse_env_creds(content: &str) -> Option<TcCredential> {
    let id = parse_env_value(content, "TENCENT_SECRET_ID")?;
    let key = parse_env_value(content, "TENCENT_SECRET_KEY")?;
    Some(TcCredential { id, key })
}

/// 从 `.env` 内容提取 `KEY=VALUE`（忽略注释行/空行；值去首尾空白与成对引号）。
/// 首行 BOM 剥离（set-tencent-key.ps1 以 UTF-8 BOM 写出，PowerShell 5.1 回读兼容）。
/// 找不到或值为空返回 None（调用方转为「未配置」引导，不 panic）。
/// （原居 tunnel.rs，随通道框架解体迁入——spec 008 D2）
pub fn parse_env_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim().trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if k.trim() != key {
            continue;
        }
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(v);
        return if v.is_empty() { None } else { Some(v.to_string()) };
    }
    None
}

// ── TC3-HMAC-SHA256 签名（纯函数）──────────────────────────────────────────

type HmacSha256 = Hmac<Sha256>;

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 任意长度密钥");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// SHA256 hex（公开：mesh.rs 资源完整性校验复用）
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
/// 优于删除/重建：记录 ID 保留、完全可逆、不产生重建垃圾）——服务于通道
/// 互切；**停用/mesh 终态**场景取删除/改值语义（spec 007 §3.4：停用是消除
/// 暴露面而非可逆切换）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOp {
    /// 启用/暂停一条既有记录（ModifyRecordStatus）
    SetStatus { record_id: u64, enable: bool },
    /// 新建记录（CreateRecord；仅目标记录不存在时）
    Create { rtype: String, value: String },
    /// 改既有记录的值（ModifyRecord 全量更新，type/line 原样保留——
    /// spec 007：A 记录公网 IP ⇄ 虚拟 IP）
    UpdateValue { record_id: u64, rtype: String, value: String },
    /// 彻底删除（DeleteRecord——spec 007 停用/mesh 终态的 CNAME 清理）
    Delete { record_id: u64 },
}

/// DNS 目标状态（spec 008 收敛：组网单通道）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsTarget {
    /// 组网：A → 虚拟 IP（upsert），CNAME 全删（穿透残留的安全收敛）
    Mesh(String),
}

/// 调和：把 `sub` 记录集合推向目标状态（纯函数，幂等——已满足则空操作集）。
/// `Mesh`（组网，spec 007 AC6/AC7）：CNAME **全删**（mesh 是安全收敛终态，
/// 残留 CNAME 保留可重启的公网旁路）；A 第一条 upsert 值=虚拟 IP（值对则
/// 仅确保启用），多余 A 暂停，缺失新建
pub fn reconcile(current: &[DnsRecord], target: &DnsTarget) -> Vec<RecordOp> {
    let mut ops = Vec::new();
    let norm = |s: &str| s.trim_end_matches('.').to_ascii_lowercase();
    let is = |r: &DnsRecord, t: &str| r.rtype.eq_ignore_ascii_case(t);
    match target {
        DnsTarget::Mesh(vip) => {
            // CNAME 全删（不管值与状态）
            for r in current.iter().filter(|r| is(r, "CNAME")) {
                ops.push(RecordOp::Delete { record_id: r.record_id });
            }
            // A：第一条 upsert 到虚拟 IP，多余暂停，缺失新建
            let a_records: Vec<&DnsRecord> = current.iter().filter(|r| is(r, "A")).collect();
            match a_records.first() {
                Some(r) => {
                    if !norm(&r.value).eq_ignore_ascii_case(norm(vip).as_str()) {
                        ops.push(RecordOp::UpdateValue {
                            record_id: r.record_id,
                            rtype: "A".into(),
                            value: vip.clone(),
                        });
                    }
                    if !r.enabled {
                        ops.push(RecordOp::SetStatus { record_id: r.record_id, enable: true });
                    }
                    for r in a_records.iter().skip(1) {
                        if r.enabled {
                            ops.push(RecordOp::SetStatus { record_id: r.record_id, enable: false });
                        }
                    }
                }
                None => ops.push(RecordOp::Create { rtype: "A".into(), value: vip.clone() }),
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

/// 改记录值（ModifyRecord 全量更新：type/line 原样传回，仅 value 变）
fn modify_record(
    cred: &TcCredential,
    root: &str,
    sub: &str,
    record_id: u64,
    rtype: &str,
    value: &str,
) -> Result<(), String> {
    call_api(
        cred,
        "ModifyRecord",
        &serde_json::json!({ "Domain": root, "SubDomain": sub, "RecordId": record_id, "RecordType": rtype, "RecordLine": "默认", "Value": value }),
    )
    .map(|_| ())
}

/// 删除记录（DeleteRecord——停用/mesh 终态清理）
fn delete_record(cred: &TcCredential, root: &str, record_id: u64) -> Result<(), String> {
    call_api(
        cred,
        "DeleteRecord",
        &serde_json::json!({ "Domain": root, "RecordId": record_id }),
    )
    .map(|_| ())
}

// ── 高层同步（mesh_sync_dns 命令消费）───────────────────────────────────────

/// 应用一个调和操作集（幂等；返回应用条数）
fn apply_ops(cred: &TcCredential, root: &str, sub: &str, ops: Vec<RecordOp>) -> Result<usize, String> {
    let mut applied = 0;
    for op in ops {
        match op {
            RecordOp::SetStatus { record_id, enable } => set_record_status(cred, root, record_id, enable)?,
            RecordOp::Create { rtype, value } => create_record(cred, root, sub, &rtype, &value)?,
            RecordOp::UpdateValue { record_id, rtype, value } => {
                modify_record(cred, root, sub, record_id, &rtype, &value)?
            }
            RecordOp::Delete { record_id } => delete_record(cred, root, record_id)?,
        }
        applied += 1;
    }
    Ok(applied)
}

/// 同步到组网（spec 007 AC6/AC7）：CNAME 全删 + A upsert 值=虚拟 IP（幂等）。
/// 虚拟 IP 是私网段——公网不可路由，达成「公网解析仅指向私网段」（AC7）。
/// CNAME 残留清理（D6：卸载后兜底）复用本函数的 reconcile 删除语义。
pub fn sync_to_mesh(cred: &TcCredential, root: &str, sub: &str, virtual_ip: &str) -> Result<usize, String> {
    let current = list_records(cred, root, sub)?;
    apply_ops(cred, root, sub, reconcile(&current, &DnsTarget::Mesh(virtual_ip.to_string())))
}

// ── DNS 对齐判定（spec 008 自 tunnel.rs 迁入；AC8：A=虚拟 IP）──────────────

/// DNS 对齐结论（指引条显隐的数据源）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DnsAlignment {
    /// 无 CNAME 且 A 记录 = 虚拟 IP → 组网通道解析就绪（spec 007 体检重定义）
    AlignedMesh,
    /// 存在 CNAME 但目标不符（穿透残留旁路，detail 带实际目标）
    MismatchedCname { actual: String },
    /// 存在 A 记录但值 ≠ 虚拟 IP（公网 IP 残留 = 切组网未完成；spec 007）
    MismatchedA { actual: String },
    /// 既无 CNAME 也无 A（用户删了记录还没配好新值）
    NoRecord,
    /// 查询本身失败（网络/解析器异常），不构成结论
    QueryFailed,
}

/// 组网态对齐判定（spec 007 体检重定义，plan §5.1：DNS 对齐 → A 记录 = 虚拟 IP）。
/// 空串视为无记录（PowerShell `[string]$null` 产出 ""，见 commands::run_dns_probe）。
pub fn judge_dns_mesh(
    cname_target: Option<&str>,
    a_value: Option<&str>,
    virtual_ip: &str,
) -> DnsAlignment {
    let cname = cname_target.map(str::trim).filter(|s| !s.is_empty());
    if let Some(target) = cname {
        // 组网态不该有 CNAME（切组网时已全删）——残留即旁路暴露面
        return DnsAlignment::MismatchedCname { actual: target.to_string() };
    }
    match a_value.map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) if v.eq_ignore_ascii_case(virtual_ip.trim()) => DnsAlignment::AlignedMesh,
        Some(v) => DnsAlignment::MismatchedA { actual: v.to_string() },
        None => DnsAlignment::NoRecord,
    }
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
        let env = "SAMPLE_KEY=x\nTENCENT_SECRET_ID=AKIDtest\nTENCENT_SECRET_KEY=keytest\n";
        let cred = parse_env_creds(env).expect("应有凭证");
        assert_eq!(cred.id, "AKIDtest");
        assert_eq!(cred.key, "keytest");
        assert_eq!(parse_env_creds("TENCENT_SECRET_ID=only"), None, "缺任一即缺失");
    }

    /// parse_env_value（原 tunnel.rs 迁入，spec 008 D2）：BOM/引号/空白容忍
    #[test]
    fn env_value_parsing_tolerates_bom_quotes_and_spaces() {
        let env = "SAMPLE_KEY=abc123\nOTHER=x\nEMPTY=\nQUOTED=\"hi there\"\n";
        assert_eq!(parse_env_value(env, "SAMPLE_KEY").as_deref(), Some("abc123"));
        assert_eq!(parse_env_value(env, "OTHER").as_deref(), Some("x"));
        assert_eq!(parse_env_value(env, "EMPTY"), None, "空值视为未配置");
        assert_eq!(parse_env_value(env, "QUOTED").as_deref(), Some("hi there"));
        assert_eq!(parse_env_value(env, "MISSING"), None);
        assert_eq!(parse_env_value("SAMPLE_KEY = spaced ", "SAMPLE_KEY").as_deref(), Some("spaced"));
        assert_eq!(
            parse_env_value("\u{feff}# comment\nTENCENT_SECRET_ID=bommed", "TENCENT_SECRET_ID").as_deref(),
            Some("bommed"),
            "UTF-8 BOM 剥离（set-tencent-key.ps1 写出形态）"
        );
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

    /// 组网对齐判定（原 tunnel.rs 迁入，spec 008 D2）
    #[test]
    fn judge_dns_mesh_covers_all_outcomes() {
        use DnsAlignment::*;
        assert_eq!(judge_dns_mesh(None, Some("10.126.126.1"), "10.126.126.1"), AlignedMesh);
        assert_eq!(
            judge_dns_mesh(None, Some("113.87.11.22"), "10.126.126.1"),
            MismatchedA { actual: "113.87.11.22".into() }
        );
        assert_eq!(
            judge_dns_mesh(Some("legacy-cname.example.net"), Some("10.126.126.1"), "10.126.126.1"),
            MismatchedCname { actual: "legacy-cname.example.net".into() }
        );
        assert_eq!(judge_dns_mesh(None, None, "10.126.126.1"), NoRecord);
        // 空串视为无记录（PowerShell [string]$null 产出 ""）
        assert_eq!(judge_dns_mesh(Some(""), Some(""), "10.126.126.1"), NoRecord);
    }

    /// spec 007 AC6/AC7：切组网 = CNAME 全删（含暂停的残留）+ A upsert 虚拟 IP。
    /// 混挂实测形态（穿透在用：A 活跃公网 IP + CNAME 活跃）→ 删 CNAME、
    /// A 改值（UpdateValue 保留记录 ID）
    #[test]
    fn reconcile_mesh_deletes_cname_and_upserts_virtual_ip() {
        let current = vec![
            DnsRecord { record_id: 1, rtype: "A".into(), value: "117.182.118.202".into(), enabled: true },
            DnsRecord { record_id: 2, rtype: "CNAME".into(), value: "legacy-cname.example.net".into(), enabled: true },
            DnsRecord { record_id: 3, rtype: "CNAME".into(), value: "old-node.com".into(), enabled: false },
        ];
        let ops = reconcile(&current, &DnsTarget::Mesh("10.126.126.1".into()));
        assert_eq!(
            ops,
            vec![
                RecordOp::Delete { record_id: 2 },
                RecordOp::Delete { record_id: 3 },
                RecordOp::UpdateValue { record_id: 1, rtype: "A".into(), value: "10.126.126.1".into() },
            ],
            "CNAME 全删（活跃+暂停）→ A 改值（已启用无需 SetStatus）"
        );

        // A 已是虚拟 IP 但被暂停 → 仅激活；已对齐 → 幂等空操作
        let paused = vec![DnsRecord { record_id: 4, rtype: "A".into(), value: "10.126.126.1".into(), enabled: false }];
        assert_eq!(
            reconcile(&paused, &DnsTarget::Mesh("10.126.126.1".into())),
            vec![RecordOp::SetStatus { record_id: 4, enable: true }]
        );
        let aligned = vec![DnsRecord { record_id: 5, rtype: "A".into(), value: "10.126.126.1".into(), enabled: true }];
        assert!(reconcile(&aligned, &DnsTarget::Mesh("10.126.126.1".into())).is_empty());

        // 无 A → 新建（A, 虚拟 IP）；值错且暂停 → 改值 + 激活两步
        assert_eq!(
            reconcile(&[], &DnsTarget::Mesh("10.126.126.1".into())),
            vec![RecordOp::Create { rtype: "A".into(), value: "10.126.126.1".into() }]
        );
        let wrong_paused = vec![DnsRecord { record_id: 6, rtype: "A".into(), value: "1.2.3.4".into(), enabled: false }];
        assert_eq!(
            reconcile(&wrong_paused, &DnsTarget::Mesh("10.126.126.1".into())),
            vec![
                RecordOp::UpdateValue { record_id: 6, rtype: "A".into(), value: "10.126.126.1".into() },
                RecordOp::SetStatus { record_id: 6, enable: true },
            ]
        );

        // 多条 A：第一条 upsert，多余暂停（防多条活跃混乱）
        let multi = vec![
            DnsRecord { record_id: 7, rtype: "A".into(), value: "1.1.1.1".into(), enabled: true },
            DnsRecord { record_id: 8, rtype: "A".into(), value: "2.2.2.2".into(), enabled: true },
        ];
        assert_eq!(
            reconcile(&multi, &DnsTarget::Mesh("10.126.126.1".into())),
            vec![
                RecordOp::UpdateValue { record_id: 7, rtype: "A".into(), value: "10.126.126.1".into() },
                RecordOp::SetStatus { record_id: 8, enable: false },
            ]
        );
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
