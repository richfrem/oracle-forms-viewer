use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_db::MockDriver;
use oracle_forms_runtime::{FormSession, FormsRecordStatus};
use oracle_forms_xml::FormsXmlParser;
use std::path::PathBuf;

fn resolve_sample(name: &str) -> PathBuf {
    let prefixes = ["", ".", "..", "../..", "../../.."];
    let subdirs = ["samples", "samples/conformance", "conformance"];
    for p in &prefixes {
        for s in &subdirs {
            let candidate = if p.is_empty() {
                PathBuf::from(s).join(name)
            } else {
                PathBuf::from(p).join(s).join(name)
            };
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    panic!("Sample file {} not found in candidate paths", name);
}

#[tokio::test]
async fn test_master_detail_coordination_orders() {
    let key = ModuleKey::new("ORDERS", ModuleType::Form);
    let sample = resolve_sample("orders_fmb.xml");
    let cache_dir = std::env::temp_dir().join("orders_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir)
        .expect("Failed to parse orders_fmb XML");

    let mut session = FormSession::new(module);
    session.start().expect("Session startup failed");

    // Verify master-detail links loaded
    assert_eq!(session.master_detail_links.len(), 1);
    let link = session.master_detail_links[0].clone();
    assert_eq!(link.master_block, "ORDERS");
    assert_eq!(link.detail_block, "ORDER_LINES");

    // Populate master record in ORDERS
    session
        .set_item_value("ORDERS.ORDER_ID", serde_json::Value::from(1001))
        .expect("Failed to set ORDER_ID");

    // Verify detail WHERE clause generation
    let master_rec = session.blocks.get("ORDERS").unwrap().current_record().unwrap();
    let detail_where = link.evaluate_detail_where(&master_rec.values);
    assert!(detail_where.contains("ORDER_LINES.ORDER_ID = 1001"));
    assert!(detail_where.contains("ORDER_LINES.CANCELLED = 'N'"));

    // Execute block query with MockDriver
    let mock = MockDriver::new().expect("Failed to init MockDriver");
    let res = session
        .execute_query("ORDERS", Some(&mock))
        .await
        .expect("Block query failed");

    assert!(!res.records.is_empty());
    let orders_buf = session.blocks.get("ORDERS").unwrap();
    assert_eq!(orders_buf.status(), FormsRecordStatus::Query);
}
