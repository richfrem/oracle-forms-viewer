use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_db::{DatabaseDriver, MockDriver, QueryRecord, QueryResult};
use oracle_forms_runtime::{
    FormSession, FormsRecordStatus, OracleValue, RuntimeError,
};
use oracle_forms_xml::FormsXmlParser;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
fn test_conf_08_commit_pipeline_full_crud_event_sequence() {
    let key = ModuleKey::new("CONF_08_COMMIT_PIPELINE", ModuleType::Form);
    let sample = resolve_sample("conf_08_commit_pipeline_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_08_cache_crud");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir)
        .expect("Failed to parse conf_08_commit_pipeline XML");

    let mut session = FormSession::new(module);
    let mock_driver = Arc::new(MockDriver::new().expect("Failed to initialize MockDriver"));
    session.set_database_driver(mock_driver);

    session.start().expect("Session startup failed");
    session.trace_log.clear();

    // 1. Setup 3 records in ITEMS buffer:
    // - Record 1: Queried, then modified (targeted for UPDATE)
    // - Record 2: Queried, then marked for delete (targeted for DELETE)
    // - Record 3: Newly created (targeted for INSERT with sequence PK)

    let mut rec1_vals = HashMap::new();
    rec1_vals.insert("ITEM_ID".to_string(), serde_json::json!(101));
    rec1_vals.insert("ITEM_NAME".to_string(), serde_json::json!("Widget A"));
    rec1_vals.insert("AMOUNT".to_string(), serde_json::json!(50.0));

    let mut rec2_vals = HashMap::new();
    rec2_vals.insert("ITEM_ID".to_string(), serde_json::json!(102));
    rec2_vals.insert("ITEM_NAME".to_string(), serde_json::json!("Widget B"));
    rec2_vals.insert("AMOUNT".to_string(), serde_json::json!(75.0));

    let qr = QueryResult {
        block_name: "ITEMS".to_string(),
        table_name: "ITEMS".to_string(),
        columns: vec!["ITEM_ID".to_string(), "ITEM_NAME".to_string(), "AMOUNT".to_string()],
        records: vec![
            QueryRecord {
                row_id: Some("101".to_string()),
                values: rec1_vals,
            },
            QueryRecord {
                row_id: Some("102".to_string()),
                values: rec2_vals,
            },
        ],
        total_count: 2,
        execution_time_ms: 1,
        sql_statement: "SELECT ROWID, ITEM_ID, ITEM_NAME, AMOUNT FROM ITEMS".to_string(),
    };

    if let Some(buf) = session.blocks.get_mut("ITEMS") {
        buf.populate_from_query_result(qr);
    }

    // Mutate Record 1 (Queried -> Changed)
    if let Some(buf) = session.blocks.get_mut("ITEMS") {
        buf.set_cursor(0);
    }
    session
        .set_item_value("ITEMS.AMOUNT", serde_json::json!(99.9))
        .expect("Failed to edit Record 1");

    // Mark Record 2 for deletion
    if let Some(buf) = session.blocks.get_mut("ITEMS") {
        buf.set_cursor(1);
        buf.delete_record();
    }

    // Create Record 3 (New -> Insert)
    session.create_record().expect("Failed to create record");
    session
        .set_item_value("ITEMS.ITEM_NAME", serde_json::json!("New Widget C"))
        .expect("Failed to set Record 3 name");
    session
        .set_item_value("ITEMS.AMOUNT", serde_json::json!(120.0))
        .expect("Failed to set Record 3 amount");

    // Clear trace log to capture the commit pipeline exclusively
    session.trace_log.clear();

    // 2. Execute Commit Form Pipeline!
    session.commit().expect("Commit pipeline execution failed");

    // 3. Verify exact canonical event firing sequence:
    // COMMIT_FORM -> PRE-COMMIT ->
    // (Per-record processing in buffer order):
    // Record 1 (UPDATE): PRE-UPDATE(ITEMS) -> POST-UPDATE(ITEMS)
    // Record 2 (DELETE): PRE-DELETE(ITEMS) -> POST-DELETE(ITEMS)
    // Record 3 (INSERT): PRE-INSERT(ITEMS) -> POST-INSERT(ITEMS)
    // -> POST-FORMS-COMMIT
    let events: Vec<&str> = session
        .trace_log
        .iter()
        .filter(|s| !s.starts_with("BUILTIN: "))
        .map(|s| s.as_str())
        .collect();

    assert_eq!(
        events,
        vec![
            "COMMIT_FORM",
            "PRE-COMMIT",
            "PRE-UPDATE(ITEMS)",
            "MESSAGE: Item updating",
            "POST-UPDATE(ITEMS)",
            "MESSAGE: Item updated",
            "PRE-DELETE(ITEMS)",
            "MESSAGE: Item deleting",
            "POST-DELETE(ITEMS)",
            "MESSAGE: Item deleted",
            "PRE-INSERT(ITEMS)",
            "POST-INSERT(ITEMS)",
            "MESSAGE: Item inserted",
            "POST-FORMS-COMMIT",
            "MESSAGE: Transaction committed.",
        ]
    );

    // 4. Verify primary key sequence was generated during PRE-INSERT:
    // PRE-INSERT trigger: :ITEMS.ITEM_ID := :SEQUENCE.ITEM_SEQ.NEXTVAL;
    let buf = session.blocks.get("ITEMS").unwrap();
    // Record 2 was deleted, so buf.records has 2 records now:
    assert_eq!(buf.records.len(), 2);

    // Record 1 updated and transitioned to QUERY
    assert_eq!(buf.records[0].status, FormsRecordStatus::Query);
    assert_eq!(buf.records[0].get_value("AMOUNT"), Some(&serde_json::json!(99.9)));

    // Record 2 (newly inserted) received sequence primary key and transitioned to QUERY
    assert_eq!(buf.records[1].status, FormsRecordStatus::Query);
    assert_eq!(buf.records[1].get_value("ITEM_NAME"), Some(&serde_json::json!("New Widget C")));
    assert_eq!(buf.records[1].get_value("ITEM_ID"), Some(&serde_json::json!(1001.0)));

    // Global commit phase was updated by triggers
    assert_eq!(
        session.get_bind_value("GLOBAL.COMMIT_PHASE"),
        OracleValue::varchar2("COMMITTED")
    );
}

#[test]
fn test_conf_08_first_edit_locking_and_concurrency_conflict() {
    let key = ModuleKey::new("CONF_08_COMMIT_PIPELINE", ModuleType::Form);
    let sample = resolve_sample("conf_08_commit_pipeline_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_08_cache_lock");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).unwrap();
    let module_clone = module.clone();

    let mut session = FormSession::new(module);
    let mock_driver = Arc::new(MockDriver::new().expect("Failed to initialize MockDriver"));
    let driver_clone = mock_driver.clone();
    session.set_database_driver(mock_driver);
    session.start().unwrap();

    // Populate a record with row_id "555" and initial values matching DB
    let mut rec_vals = HashMap::new();
    rec_vals.insert("ITEM_ID".to_string(), serde_json::json!(555));
    rec_vals.insert("ITEM_NAME".to_string(), serde_json::json!("Original"));
    rec_vals.insert("AMOUNT".to_string(), serde_json::json!(10.0));

    let qr = QueryResult {
        block_name: "ITEMS".to_string(),
        table_name: "ITEMS".to_string(),
        columns: vec!["ITEM_ID".to_string(), "ITEM_NAME".to_string(), "AMOUNT".to_string()],
        records: vec![QueryRecord {
            row_id: Some("555".to_string()),
            values: rec_vals,
        }],
        total_count: 1,
        execution_time_ms: 1,
        sql_statement: "SELECT ROWID, ITEM_ID, ITEM_NAME, AMOUNT FROM ITEMS".to_string(),
    };

    if let Some(buf) = session.blocks.get_mut("ITEMS") {
        buf.populate_from_query_result(qr);
    }

    // 1. First edit: locks record and transitions to Changed
    session
        .set_item_value("ITEMS.AMOUNT", serde_json::json!(25.0))
        .expect("First edit should lock successfully");

    let buf = session.blocks.get("ITEMS").unwrap();
    assert_eq!(buf.records[0].status, FormsRecordStatus::Changed);
    assert!(buf.records[0].is_locked);

    // 2. Concurrency Conflict: DB row was updated concurrently by another user
    // Simulate by modifying DB item 555 to amount = 999.0
    let update_sql = oracle_forms_db::BoundSql::new("UPDATE items SET amount = 999.0 WHERE item_id = 555");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let _ = rt.block_on(driver_clone.query(&update_sql));

    // A second session has initial_values where amount was 10.0
    let mut session2 = FormSession::new(module_clone);
    session2.set_database_driver(driver_clone);
    session2.start().unwrap();

    let mut rec2_vals = HashMap::new();
    rec2_vals.insert("ITEM_ID".to_string(), serde_json::json!(555));
    rec2_vals.insert("ITEM_NAME".to_string(), serde_json::json!("Original"));
    rec2_vals.insert("AMOUNT".to_string(), serde_json::json!(10.0));

    let qr2 = QueryResult {
        block_name: "ITEMS".to_string(),
        table_name: "ITEMS".to_string(),
        columns: vec!["ITEM_ID".to_string(), "ITEM_NAME".to_string(), "AMOUNT".to_string()],
        records: vec![QueryRecord {
            row_id: Some("555".to_string()),
            values: rec2_vals,
        }],
        total_count: 1,
        execution_time_ms: 1,
        sql_statement: "SELECT ROWID, ITEM_ID, ITEM_NAME, AMOUNT FROM ITEMS".to_string(),
    };

    if let Some(buf) = session2.blocks.get_mut("ITEMS") {
        buf.populate_from_query_result(qr2);
    }

    // Session 2 tries to edit: concurrency check detects DB has changed -> fails with FRM-40501!
    let edit_res = session2.set_item_value("ITEMS.AMOUNT", serde_json::json!(42.0));
    assert!(
        matches!(edit_res, Err(RuntimeError::RecordLockedOrChanged(_))),
        "Expected RecordLockedOrChanged concurrency error, got: {:?}",
        edit_res
    );
}

#[test]
fn test_conf_08_pre_commit_failure_triggers_savepoint_rollback() {
    let key = ModuleKey::new("CONF_08_COMMIT_PIPELINE", ModuleType::Form);
    let sample = resolve_sample("conf_08_commit_pipeline_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_08_cache_rollback");
    let mut module = FormsXmlParser::parse(&key, &sample, &cache_dir).unwrap();

    // Inject failing PRE-COMMIT trigger
    for t in &mut module.triggers {
        if t.name == "PRE-COMMIT" {
            t.first_line = "BEGIN RAISE FORM_TRIGGER_FAILURE; END;".to_string();
            t.source_text = Some("BEGIN RAISE FORM_TRIGGER_FAILURE; END;".to_string());
        }
    }

    let mut session = FormSession::new(module);
    session.start().unwrap();

    // Create a dirty record
    session.create_record().unwrap();
    session.set_item_value("ITEMS.ITEM_NAME", serde_json::json!("Failing Item")).unwrap();

    session.trace_log.clear();

    // Attempt commit: must fail and record ROLLBACK_TO_SAVEPOINT
    let res = session.commit();
    assert!(matches!(res, Err(RuntimeError::TriggerFailure(_))));

    assert!(session.trace_log.contains(&"ROLLBACK_TO_SAVEPOINT".to_string()));
    // Post-commit must NOT have fired
    assert!(!session.trace_log.contains(&"POST-FORMS-COMMIT".to_string()));
}
