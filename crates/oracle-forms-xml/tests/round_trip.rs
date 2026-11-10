use oracle_forms_core::diff::FormDiffer;
use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_xml::{FormsXmlParser, FormsXmlWriter};
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

#[test]
fn test_xml_round_trip_and_diff_fidelity() {
    let key = ModuleKey::new("ORDERS", ModuleType::Form);
    let sample = resolve_sample("orders_fmb.xml");
    let cache_dir = std::env::temp_dir().join("round_trip_cache");
    let original = FormsXmlParser::parse(&key, &sample, &cache_dir)
        .expect("Failed to parse orders_fmb.xml");

    let mut modified = original.clone();

    // 1. Mutate item coordinates and prompt in block ORDERS
    let orders_block = modified
        .blocks
        .iter_mut()
        .find(|b| b.name == "ORDERS")
        .expect("Block ORDERS not found");

    let order_id_item = orders_block
        .items
        .iter_mut()
        .find(|i| i.name == "ORDER_ID")
        .expect("Item ORDER_ID not found");

    order_id_item.x_position = 45;
    order_id_item.y_position = 75;
    order_id_item.width = Some(90);
    order_id_item.height = Some(22);
    order_id_item.prompt = Some("Purchase Order Number".to_string());

    // 2. Add a where clause to block ORDERS
    if let Some(dml) = &mut orders_block.dml {
        dml.where_clause = Some("STATUS = 'PENDING'".to_string());
    } else {
        orders_block.dml = Some(oracle_forms_core::model::BlockDml {
            where_clause: Some("STATUS = 'PENDING'".to_string()),
            ..Default::default()
        });
    }

    // 3. Verify FormDiffer detects exactly these mutations
    let diff = FormDiffer::diff(&original, &modified);
    assert!(diff.has_differences());

    let block_diff = diff
        .blocks_modified
        .iter()
        .find(|b| b.name == "ORDERS")
        .expect("ORDERS block diff expected");

    let item_diff = block_diff
        .items_modified
        .iter()
        .find(|i| i.name == "ORDER_ID")
        .expect("ORDER_ID item diff expected");

    let geom = item_diff.geometry_diff.as_ref().expect("Geometry change expected");
    assert_eq!(geom.new_x, 45);
    assert_eq!(geom.new_y, 75);
    assert_eq!(geom.new_width, Some(90));
    assert_eq!(geom.new_height, Some(22));

    let prompt_change = item_diff
        .property_changes
        .iter()
        .find(|p| p.property == "prompt")
        .expect("Prompt change expected");
    assert_eq!(prompt_change.new_value, Some("Purchase Order Number".to_string()));

    // 4. Serialize to Forms2XML XML string
    let xml_output = FormsXmlWriter::write_string(&modified);
    assert!(xml_output.contains("Purchase Order Number"));
    assert!(xml_output.contains("XPosition=\"45\""));
    assert!(xml_output.contains("YPosition=\"75\""));
    assert!(xml_output.contains("WhereClause=\"STATUS = 'PENDING'\""));

    // 5. Parse back the serialized XML
    let temp_xml_file = std::env::temp_dir().join("round_trip_orders.xml");
    std::fs::write(&temp_xml_file, &xml_output).expect("Failed to write temp xml");

    let reparsed = FormsXmlParser::parse(&key, &temp_xml_file, &cache_dir)
        .expect("Failed to re-parse generated XML");

    // 6. Verify mutations survived round-trip
    let rep_orders = reparsed
        .blocks
        .iter()
        .find(|b| b.name == "ORDERS")
        .expect("Reparsed ORDERS block not found");

    let rep_item = rep_orders
        .items
        .iter()
        .find(|i| i.name == "ORDER_ID")
        .expect("Reparsed ORDER_ID item not found");

    assert_eq!(rep_item.x_position, 45);
    assert_eq!(rep_item.y_item_position_or_coord(), 75);
    assert_eq!(rep_item.width, Some(90));
    assert_eq!(rep_item.height, Some(22));
    assert_eq!(rep_item.prompt.as_deref(), Some("Purchase Order Number"));
    assert_eq!(
        rep_orders.dml.as_ref().and_then(|d| d.where_clause.as_deref()),
        Some("STATUS = 'PENDING'")
    );

    let _ = std::fs::remove_file(&temp_xml_file);
}

trait ItemExt {
    fn y_item_position_or_coord(&self) -> i32;
}

impl ItemExt for oracle_forms_core::model::ItemInfo {
    fn y_item_position_or_coord(&self) -> i32 {
        self.y_position
    }
}
