//! 一次性真机验证（spec 004 DNS 自动切换）：读取本机凭证 → 列出 ai 子域记录 →
//! 调和到穿透目标态（A 暂停、CNAME 激活/新建，暂停/激活语义幂等）→ 回读验证。
//! 用法：cargo run --example dns_sync_probe

use ai_remote_workbench_lib::dns_api;

fn main() {
    let Some(cred) = dns_api::read_credential() else {
        eprintln!("未找到腾讯云凭证（.env / ddns-go.yaml）");
        std::process::exit(1);
    };
    println!("凭证已读取（id 前缀：{}…）", &cred.id[..cred.id.len().min(8)]);

    let (root, sub) = ("jackqi.cn", "ai");
    println!("切换前记录：");
    for r in dns_api::list_records_probe(&cred, root, sub) {
        println!("  [{:5}|{}] {}", r.rtype, if r.enabled { "ON " } else { "OFF" }, r.value);
    }

    match dns_api::sync_to_tunnel(&cred, root, sub, "frp-can.com") {
        Ok(n) => println!("调和完成：应用 {n} 条操作"),
        Err(e) => {
            eprintln!("同步失败：{e}");
            std::process::exit(2);
        }
    }

    println!("切换后记录：");
    for r in dns_api::list_records_probe(&cred, root, sub) {
        println!("  [{:5}|{}] {}", r.rtype, if r.enabled { "ON " } else { "OFF" }, r.value);
    }
}
