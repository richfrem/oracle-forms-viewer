use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_runtime::{FormSession, RuntimeError};
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

#[test]
fn test_conf_02_nav_valid_lifecycle() {
    let key = ModuleKey::new("CONF_02_NAV_VALID", ModuleType::Form);
    let sample = resolve_sample("conf_02_nav_valid_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_02_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir)
        .expect("Failed to parse conf_02_nav_valid XML");

    let mut session = FormSession::new(module);
    session.start().expect("Session startup failed");

    // Clear startup trace log to isolate navigation events
    session.trace_log.clear();

    // 1. Clean Navigation: moving from ITEM1 to ITEM2 without modifying ITEM1
    // WHEN-VALIDATE-ITEM MUST NOT FIRE when the field is untouched
    session.go_item("BLOCK1.ITEM2").expect("Navigation to ITEM2 failed");

    assert_eq!(
        session.trace_log,
        vec![
            "PRE-TEXT-ITEM(BLOCK1.ITEM2)",
            "WHEN-NEW-ITEM-INSTANCE(BLOCK1.ITEM2)",
        ]
    );
    assert_eq!(session.system.get_var("CURSOR_ITEM").as_deref(), Some("BLOCK1.ITEM2"));

    // 2. Set validation hook: simulate WHEN-VALIDATE-ITEM failing if value is "INVALID"
    session.set_validation_hook(Box::new(|_block, _item, val| {
        if let Some(s) = val.as_str() {
            if s == "INVALID" {
                return Err("FRM-40735: WHEN-VALIDATE-ITEM raised unhandled exception".to_string());
            }
        }
        Ok(())
    }));

    // 3. Mutate ITEM2 with an invalid value
    session.trace_log.clear();
    session
        .set_item_value("BLOCK1.ITEM2", serde_json::Value::from("INVALID"))
        .expect("Failed to set item value");

    // Attempt to navigate to ITEM1: WHEN-VALIDATE-ITEM must fire and abort navigation!
    let nav_err = session.go_item("BLOCK1.ITEM1");
    assert!(nav_err.is_err());
    match nav_err.unwrap_err() {
        RuntimeError::ValidationFailed { item, message } => {
            assert_eq!(item, "BLOCK1.ITEM2");
            assert!(message.contains("WHEN-VALIDATE-ITEM"));
        }
        e => panic!("Expected ValidationFailed error, got: {:?}", e),
    }

    // Cursor MUST remain on BLOCK1.ITEM2 after validation abort
    assert_eq!(session.system.get_var("CURSOR_ITEM").as_deref(), Some("BLOCK1.ITEM2"));

    // Trace log must contain POST-TEXT-ITEM -> WHEN-VALIDATE-ITEM and failure notice
    assert_eq!(
        session.trace_log,
        vec![
            "POST-TEXT-ITEM(BLOCK1.ITEM2)",
            "WHEN-VALIDATE-ITEM(BLOCK1.ITEM2)",
            "TRIGGER_FAILURE: FRM-40735: WHEN-VALIDATE-ITEM raised unhandled exception",
        ]
    );

    // 4. Correct the value to "VALID"
    session.trace_log.clear();
    session
        .set_item_value("BLOCK1.ITEM2", serde_json::Value::from("VALID"))
        .expect("Failed to set valid value");

    // Navigate to ITEM1: validation passes, navigation proceeds!
    session.go_item("BLOCK1.ITEM1").expect("Navigation should succeed with valid value");

    assert_eq!(
        session.trace_log,
        vec![
            "POST-TEXT-ITEM(BLOCK1.ITEM2)",
            "WHEN-VALIDATE-ITEM(BLOCK1.ITEM2)",
            "PRE-TEXT-ITEM(BLOCK1.ITEM1)",
            "WHEN-NEW-ITEM-INSTANCE(BLOCK1.ITEM1)",
        ]
    );
    assert_eq!(session.system.get_var("CURSOR_ITEM").as_deref(), Some("BLOCK1.ITEM1"));
}
