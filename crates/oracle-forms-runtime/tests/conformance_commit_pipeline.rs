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
    let db = mock_driver.clone();
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

    // 3. Verify the Forms commit sequence: validation of changed records, PRE-COMMIT,
    // then per block DELETEs first, then UPDATE/INSERT in record order, POST-FORMS-COMMIT,
    // the database COMMIT, and POST-DATABASE-COMMIT.
    let events: Vec<&str> = session
        .trace_log
        .iter()
        .filter(|s| !s.starts_with("BUILTIN: ") && !s.starts_with("DML: ") && !s.starts_with("LOCAL_SEQUENCE"))
        .map(|s| s.as_str())
        .collect();

    assert_eq!(
        events,
        vec![
            "COMMIT_FORM",
            "WHEN-VALIDATE-ITEM(ITEMS.AMOUNT)",
            "WHEN-VALIDATE-RECORD(ITEMS)",
            "WHEN-VALIDATE-ITEM(ITEMS.ITEM_NAME)",
            "WHEN-VALIDATE-ITEM(ITEMS.AMOUNT)",
            "WHEN-VALIDATE-RECORD(ITEMS)",
            "PRE-COMMIT",
            "PRE-DELETE(ITEMS)",
            "MESSAGE: Item deleting",
            "POST-DELETE(ITEMS)",
            "MESSAGE: Item deleted",
            "PRE-UPDATE(ITEMS)",
            "MESSAGE: Item updating",
            "POST-UPDATE(ITEMS)",
            "MESSAGE: Item updated",
            "PRE-INSERT(ITEMS)",
            "POST-INSERT(ITEMS)",
            "MESSAGE: Item inserted",
            "POST-FORMS-COMMIT",
            "MESSAGE: Transaction committed.",
            "DATABASE_COMMIT",
            "POST-DATABASE-COMMIT",
        ]
    );

    // The DML really reached the database.
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let rows = rt
        .block_on(db.query(&oracle_forms_db::BoundSql::new("SELECT item_id, item_name, amount FROM items ORDER BY item_id")))
        .unwrap();
    let names: Vec<String> = rows.records.iter().map(|r| r.values["ITEM_NAME"].as_str().unwrap().to_string()).collect();
    assert!(!names.contains(&"Widget B".to_string()), "deleted row still present: {names:?}");
    assert!(names.contains(&"New Widget C".to_string()), "inserted row missing: {names:?}");
    let widget_a = rows.records.iter().find(|r| r.values["ITEM_NAME"] == "Widget A").unwrap();
    assert_eq!(widget_a.values["AMOUNT"], serde_json::json!(99.9));

    // Buffer state after commit.
    let buf = session.blocks.get("ITEMS").unwrap();
    assert_eq!(buf.records.len(), 2);
    assert!(buf.deleted_records.is_empty());
    assert_eq!(buf.records[0].status, FormsRecordStatus::Query);
    assert_eq!(buf.records[0].get_value("AMOUNT"), Some(&serde_json::json!(99.9)));
    // PRE-INSERT assigned the key from :SEQUENCE.ITEM_SEQ.NEXTVAL.
    assert_eq!(buf.records[1].status, FormsRecordStatus::Query);
    assert_eq!(buf.records[1].get_value("ITEM_NAME"), Some(&serde_json::json!("New Widget C")));
    assert_eq!(buf.records[1].get_value("ITEM_ID"), Some(&serde_json::json!(1001)));

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

/// A failing INSERT must roll back everything done earlier in the same commit and leave
/// the buffers untouched, so the user can correct the data and commit again.
#[test]
fn test_conf_08_failed_dml_rolls_back_whole_commit() {
    let key = ModuleKey::new("CONF_08_COMMIT_PIPELINE", ModuleType::Form);
    let sample = resolve_sample("conf_08_commit_pipeline_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_08_cache_dmlfail");
    let mut module = FormsXmlParser::parse(&key, &sample, &cache_dir).unwrap();
    // Make PRE-INSERT assign a key that already exists (item 101) -> PRIMARY KEY violation.
    for t in &mut module.triggers {
        if t.name == "PRE-INSERT" {
            t.source_text = Some("BEGIN :ITEMS.ITEM_ID := 101; END;".to_string());
        }
    }

    let db = Arc::new(MockDriver::new().unwrap());
    let mut session = FormSession::new(module);
    session.set_database_driver(db.clone());
    session.start().unwrap();

    let mut vals = HashMap::new();
    vals.insert("ITEM_ID".to_string(), serde_json::json!(102));
    vals.insert("ITEM_NAME".to_string(), serde_json::json!("Widget B"));
    vals.insert("AMOUNT".to_string(), serde_json::json!(75.0));
    session.blocks.get_mut("ITEMS").unwrap().populate_from_query_result(QueryResult {
        block_name: "ITEMS".into(),
        table_name: "ITEMS".into(),
        columns: vec!["ITEM_ID".into(), "ITEM_NAME".into(), "AMOUNT".into()],
        records: vec![QueryRecord { row_id: Some("102".into()), values: vals }],
        total_count: 1,
        execution_time_ms: 0,
        sql_statement: String::new(),
    });

    // Update record 1 (would succeed), then add a record whose INSERT will fail.
    session.set_item_value("ITEMS.AMOUNT", serde_json::json!(1.0)).unwrap();
    session.create_record().unwrap();
    session.set_item_value("ITEMS.ITEM_NAME", serde_json::json!("Duplicate")).unwrap();

    let res = session.commit();
    assert!(matches!(res, Err(RuntimeError::Database(_))), "expected DML failure, got {res:?}");
    assert!(session.trace_log.contains(&"ROLLBACK_TO_SAVEPOINT".to_string()));
    assert!(!session.trace_log.iter().any(|t| t == "POST-FORMS-COMMIT"));

    // The earlier UPDATE was rolled back in the database...
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let rows = rt
        .block_on(db.query(&oracle_forms_db::BoundSql::new("SELECT amount FROM items WHERE item_id = 102")))
        .unwrap();
    assert_eq!(rows.records[0].values["AMOUNT"], serde_json::json!(75.0));
    // ...and the buffer still holds the user's pending changes.
    let buf = session.blocks.get("ITEMS").unwrap();
    assert_eq!(buf.records[0].status, FormsRecordStatus::Changed);
    assert_eq!(buf.records[1].status, FormsRecordStatus::Insert);
}

#[test]
fn test_conf_08_read_only_session_refuses_commit() {
    let key = ModuleKey::new("CONF_08_COMMIT_PIPELINE", ModuleType::Form);
    let sample = resolve_sample("conf_08_commit_pipeline_fmb.xml");
    let module = FormsXmlParser::parse(&key, &sample, &std::env::temp_dir().join("conf_08_cache_ro")).unwrap();
    let mut session = FormSession::new(module);
    session.set_database_driver(Arc::new(MockDriver::new().unwrap()));
    session.read_only = true;
    session.start().unwrap();
    session.create_record().unwrap();
    session.set_item_value("ITEMS.ITEM_NAME", serde_json::json!("x")).unwrap();
    assert!(matches!(session.commit(), Err(RuntimeError::ReadOnly(_))));
}
