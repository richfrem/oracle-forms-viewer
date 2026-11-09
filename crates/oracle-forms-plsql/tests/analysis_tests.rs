use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_plsql::CodeIntelligence;
use oracle_forms_xml::FormsXmlParser;
use std::path::PathBuf;

fn resolve_sample(name: &str) -> PathBuf {
    let prefixes = ["", ".", "..", "../..", "../../.."];
    let subdirs = ["samples", "samples/conformance"];
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
fn test_code_intelligence_orders_fmb() {
    let sample = resolve_sample("orders_fmb.xml");
    let key = ModuleKey::new("ORDERS", ModuleType::Form);
    let cache_dir = std::env::temp_dir().join("analysis_orders_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).expect("Failed to parse orders_fmb.xml");

    let (call_graph, xref, report) = CodeIntelligence::analyze(&module);

    // 1. Verify Call Graph
    assert!(!call_graph.nodes.is_empty(), "Call graph nodes should not be empty");
    let has_calc_total = call_graph.nodes.iter().any(|n| n.contains("CALC_TOTAL"));
    assert!(has_calc_total, "Call graph should contain CALC_TOTAL");

    let has_commit_call = call_graph.edges.iter().any(|e| e.callee.contains("COMMIT_FORM"));
    assert!(has_commit_call, "Call graph should link KEY-COMMIT to COMMIT_FORM");

    // 2. Verify Item Cross Reference
    assert!(!xref.items.is_empty(), "Item cross references should not be empty");
    let has_order_id_ref = xref.items.keys().any(|k| k.contains("ORDER_ID"));
    assert!(has_order_id_ref, "Item cross reference should contain ORDER_ID");

    // 3. Verify Executability Report
    assert!(report.total_statements > 0, "Report should count statements");
    assert!(
        report.support_percentage >= 80.0,
        "Support percentage should be >= 80%, got {}%",
        report.support_percentage
    );
}

#[test]
fn test_code_intelligence_customers_fmb() {
    let sample = resolve_sample("customers_fmb.xml");
    let key = ModuleKey::new("CUSTOMERS", ModuleType::Form);
    let cache_dir = std::env::temp_dir().join("analysis_cust_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).expect("Failed to parse customers_fmb.xml");

    let (call_graph, xref, report) = CodeIntelligence::analyze(&module);

    assert!(!call_graph.nodes.is_empty());
    assert!(!xref.items.is_empty());
    assert!(report.total_statements > 0);
    assert!(report.support_percentage >= 80.0);
}
