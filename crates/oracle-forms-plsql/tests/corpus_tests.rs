use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_plsql::parser::Parser;
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
fn test_parse_utils_pld_corpus() {
    let sample = resolve_sample("utils.pld");
    let content = std::fs::read_to_string(&sample).expect("Failed to read utils.pld");
    let mut parser = Parser::new(&content);
    let units = parser.parse_all_units();
    assert_eq!(
        units.len(),
        4,
        "utils.pld should contain 4 units (PROCEDURE, FUNCTION, PACKAGE, PACKAGE BODY)"
    );
}

#[test]
fn test_parse_orders_fmb_triggers_and_units() {
    let sample = resolve_sample("orders_fmb.xml");
    let key = ModuleKey::new("ORDERS", ModuleType::Form);
    let cache_dir = std::env::temp_dir().join("plsql_orders_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).expect("Failed to parse orders_fmb.xml");

    // Test all triggers
    for trig in &module.triggers {
        let code = trig.source_text.as_deref().unwrap_or(&trig.first_line);
        if code.trim().is_empty() {
            continue;
        }
        let mut parser = Parser::new(code);
        let ast = parser.parse_unit();
        assert!(
            ast.is_ok(),
            "Failed to parse trigger '{}': {:?}",
            trig.name,
            ast.err()
        );
    }

    // Test all program units
    for pu in &module.program_units {
        let code = pu.source_text.as_deref().unwrap_or(&pu.first_line);
        if code.trim().is_empty() {
            continue;
        }
        let mut parser = Parser::new(code);
        let ast = parser.parse_unit();
        assert!(
            ast.is_ok(),
            "Failed to parse program unit '{}': {:?}",
            pu.name,
            ast.err()
        );
    }
}

#[test]
fn test_parse_claims_fmb_triggers() {
    let sample = resolve_sample("claims_fmb.xml");
    let key = ModuleKey::new("CLAIMS", ModuleType::Form);
    let cache_dir = std::env::temp_dir().join("plsql_claims_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).expect("Failed to parse claims_fmb.xml");

    for trig in &module.triggers {
        let code = trig.source_text.as_deref().unwrap_or(&trig.first_line);
        if code.trim().is_empty() {
            continue;
        }
        let mut parser = Parser::new(code);
        let ast = parser.parse_unit();
        assert!(
            ast.is_ok(),
            "Failed to parse claims trigger '{}': {:?}",
            trig.name,
            ast.err()
        );
    }
}

#[test]
fn test_parse_customers_fmb_triggers_and_embedded_sql() {
    let sample = resolve_sample("customers_fmb.xml");
    let key = ModuleKey::new("CUSTOMERS", ModuleType::Form);
    let cache_dir = std::env::temp_dir().join("plsql_cust_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).expect("Failed to parse customers_fmb.xml");

    for trig in &module.triggers {
        let code = trig.source_text.as_deref().unwrap_or(&trig.first_line);
        if code.trim().is_empty() {
            continue;
        }
        let mut parser = Parser::new(code);
        let ast = parser.parse_unit();
        assert!(
            ast.is_ok(),
            "Failed to parse customers trigger '{}': {:?}",
            trig.name,
            ast.err()
        );
    }
}

#[test]
fn test_parse_test_db_embedded_sql_select_into() {
    let sample = resolve_sample("test_db_fmb.xml");
    let key = ModuleKey::new("TEST_DB", ModuleType::Form);
    let cache_dir = std::env::temp_dir().join("plsql_test_db_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).expect("Failed to parse test_db_fmb.xml");

    for trig in &module.triggers {
        let code = trig.source_text.as_deref().unwrap_or(&trig.first_line);
        if code.trim().is_empty() {
            continue;
        }
        let mut parser = Parser::new(code);
        let ast = parser.parse_unit();
        assert!(
            ast.is_ok(),
            "Failed to parse test_db trigger '{}': {:?}",
            trig.name,
            ast.err()
        );
    }
}
