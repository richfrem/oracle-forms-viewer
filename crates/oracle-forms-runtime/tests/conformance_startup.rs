use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_runtime::FormSession;
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
fn test_conf_01_startup_cascade() {
    let key = ModuleKey::new("CONF_01_STARTUP", ModuleType::Form);
    let sample = resolve_sample("conf_01_startup_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_01_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir)
        .expect("Failed to parse conf_01_startup XML");

    let mut session = FormSession::new(module);
    session.start().expect("Session startup failed");

    // Exact canonical firing sequence per Oracle Forms runtime specification:
    // PRE-FORM -> PRE-BLOCK -> PRE-RECORD -> PRE-TEXT-ITEM ->
    // WHEN-NEW-FORM-INSTANCE -> WHEN-NEW-BLOCK-INSTANCE ->
    // WHEN-NEW-RECORD-INSTANCE -> WHEN-NEW-ITEM-INSTANCE
    assert_eq!(
        session.trace_log,
        vec![
            "PRE-FORM",
            "PRE-BLOCK(BLOCK1)",
            "PRE-RECORD(BLOCK1)",
            "PRE-TEXT-ITEM(BLOCK1.ITEM1)",
            "WHEN-NEW-FORM-INSTANCE",
            "WHEN-NEW-BLOCK-INSTANCE(BLOCK1)",
            "WHEN-NEW-RECORD-INSTANCE(BLOCK1)",
            "WHEN-NEW-ITEM-INSTANCE(BLOCK1.ITEM1)",
        ]
    );

    // Verify system variables
    assert_eq!(session.system.get_var("CURSOR_BLOCK").as_deref(), Some("BLOCK1"));
    assert_eq!(session.system.get_var("CURSOR_ITEM").as_deref(), Some("BLOCK1.ITEM1"));
    assert_eq!(session.system.get_var("CURSOR_RECORD").as_deref(), Some("1"));
    assert_eq!(session.system.get_var("RECORD_STATUS").as_deref(), Some("NEW"));
    assert_eq!(session.system.get_var("MODE").as_deref(), Some("NORMAL"));
}
