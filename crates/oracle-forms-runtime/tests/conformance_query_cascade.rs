use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_db::{QueryRecord, QueryResult};
use oracle_forms_plsql::ast::{BinaryOp, Statement};
use oracle_forms_runtime::{
    ALERT_BUTTON1, FormSession, InteractionRequest, InteractionResponse, Interpreter, OracleValue,
    RuntimeError,
};
use oracle_forms_xml::FormsXmlParser;
use std::collections::HashMap;
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
fn test_conf_03_query_cascade_and_post_query_calculations() {
    let key = ModuleKey::new("CONF_03_QUERY_CASCADE", ModuleType::Form);
    let sample = resolve_sample("conf_03_query_cascade_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_03_cache");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir)
        .expect("Failed to parse conf_03_query_cascade XML");

    let mut session = FormSession::new(module);
    session.start().expect("Session startup failed");

    // Clear startup trace log
    session.trace_log.clear();

    // Populate mock query records into ORDERS buffer
    let mut rec1 = HashMap::new();
    rec1.insert("ORDER_ID".to_string(), serde_json::json!(1001));
    rec1.insert("QUANTITY".to_string(), serde_json::json!(5));
    rec1.insert("UNIT_PRICE".to_string(), serde_json::json!(20.0));

    let mut rec2 = HashMap::new();
    rec2.insert("ORDER_ID".to_string(), serde_json::json!(1002));
    rec2.insert("QUANTITY".to_string(), serde_json::json!(3));
    rec2.insert("UNIT_PRICE".to_string(), serde_json::json!(50.0));

    let query_result = QueryResult {
        block_name: "ORDERS".to_string(),
        table_name: "ORDERS".to_string(),
        columns: vec![
            "ORDER_ID".to_string(),
            "QUANTITY".to_string(),
            "UNIT_PRICE".to_string(),
        ],
        records: vec![
            QueryRecord {
                row_id: None,
                values: rec1,
            },
            QueryRecord {
                row_id: None,
                values: rec2,
            },
        ],
        total_count: 2,
        execution_time_ms: 1,
        sql_statement: "SELECT ORDER_ID, QUANTITY, UNIT_PRICE FROM ORDERS".to_string(),
    };

    if let Some(buf) = session.blocks.get_mut("ORDERS") {
        buf.populate_from_query_result(query_result);
    }

    // Execute query cascade on block: PRE-QUERY -> fetch records -> POST-QUERY for each record
    session
        .execute_query_on_block("ORDERS")
        .expect("Query cascade failed");

    // 1. Verify PRE-QUERY set ORDER_STATUS to OPEN
    assert_eq!(
        session.get_bind_value("ORDERS.ORDER_STATUS"),
        OracleValue::varchar2("OPEN")
    );

    // 2. Verify POST-QUERY calculated TOTAL_AMOUNT for record 1 (5 * 20 = 100)
    let buf = session.blocks.get("ORDERS").unwrap();
    assert_eq!(
        buf.records[0].get_value("TOTAL_AMOUNT"),
        Some(&serde_json::json!(100.0))
    );

    // 3. Verify POST-QUERY calculated TOTAL_AMOUNT for record 2 (3 * 50 = 150)
    assert_eq!(
        buf.records[1].get_value("TOTAL_AMOUNT"),
        Some(&serde_json::json!(150.0))
    );

    // 4. Verify canonical trace log includes PRE-QUERY and both POST-QUERY invocations
    assert!(session.trace_log.contains(&"PRE-QUERY(ORDERS)".to_string()));
    assert_eq!(
        session
            .trace_log
            .iter()
            .filter(|&t| t == "POST-QUERY(ORDERS)")
            .count(),
        2
    );
}

#[test]
fn test_conf_03_resumable_alert_suspension_and_continuation() {
    let key = ModuleKey::new("CONF_03_QUERY_CASCADE", ModuleType::Form);
    let sample = resolve_sample("conf_03_query_cascade_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_03_cache_alert");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir)
        .expect("Failed to parse conf_03_query_cascade XML");

    let mut session = FormSession::new(module);
    session.start().expect("Session startup failed");

    // Set interactive mode so SHOW_ALERT suspends execution
    session.interactive_mode = true;
    session.set_bind_value("ORDERS.ORDER_STATUS", OracleValue::varchar2("OPEN"));

    // Fire button trigger: WHEN-BUTTON-PRESSED on BTN_DELETE
    let trigger_res = session.dispatch_event(
        "WHEN-BUTTON-PRESSED",
        Some("BTN_DELETE"),
        Some("ORDERS"),
    );

    // Trigger should have suspended!
    assert!(trigger_res.is_success());
    assert!(session.pending_suspension.is_some());

    let ctx = session.pending_suspension.as_ref().unwrap();
    match &ctx.interaction {
        InteractionRequest::ShowAlert {
            alert_name,
            message,
            button1,
            ..
        } => {
            assert_eq!(alert_name, "CONFIRM_DELETE");
            assert_eq!(message, "Are you sure you want to cancel this order?");
            assert_eq!(button1, "OK");
        }
        _ => panic!("Expected ShowAlert interaction request"),
    }

    // Status is still OPEN before user response
    assert_eq!(
        session.get_bind_value("ORDERS.ORDER_STATUS"),
        OracleValue::varchar2("OPEN")
    );

    // Now user clicks ALERT_BUTTON1 (OK)
    let resume_status = session
        .resume(InteractionResponse::AlertButton(ALERT_BUTTON1))
        .expect("Failed to resume session");

    assert!(resume_status.is_success());
    assert!(session.pending_suspension.is_none());

    // Verified: IF v_button = ALERT_BUTTON1 branch executed!
    assert_eq!(
        session.get_bind_value("ORDERS.ORDER_STATUS"),
        OracleValue::varchar2("CANCELLED")
    );
    assert_eq!(session.messages, vec!["Order cancelled."]);
}

#[test]
fn test_conf_03_oracle_value_semantics_and_three_valued_logic() {
    // 1. Invariant: empty string is strictly NULL
    let empty_v = OracleValue::varchar2("");
    assert_eq!(empty_v, OracleValue::Null);
    assert!(empty_v.is_null());

    // 2. Three-valued NULL equality: NULL = NULL evaluates to NULL (not TRUE!)
    let eq_res = OracleValue::Null.eval_binary_op(BinaryOp::Equal, &OracleValue::Null);
    assert_eq!(eq_res, OracleValue::null_boolean());
    assert!(!eq_res.is_truthy());

    // 3. Concatenation: in Oracle, NULL || 'world' is 'world'
    let concat_res = OracleValue::Null.eval_binary_op(
        BinaryOp::Concat,
        &OracleValue::varchar2("world"),
    );
    assert_eq!(concat_res, OracleValue::varchar2("world"));

    // 4. Boolean 3-valued AND: false AND NULL is false; true AND NULL is NULL
    let false_and_null = OracleValue::boolean(false).eval_binary_op(
        BinaryOp::And,
        &OracleValue::null_boolean(),
    );
    assert_eq!(false_and_null, OracleValue::boolean(false));

    let true_and_null = OracleValue::boolean(true).eval_binary_op(
        BinaryOp::And,
        &OracleValue::null_boolean(),
    );
    assert_eq!(true_and_null, OracleValue::null_boolean());

    // 5. Boolean 3-valued OR: true OR NULL is true; false OR NULL is NULL
    let true_or_null = OracleValue::boolean(true).eval_binary_op(
        BinaryOp::Or,
        &OracleValue::null_boolean(),
    );
    assert_eq!(true_or_null, OracleValue::boolean(true));

    let false_or_null = OracleValue::boolean(false).eval_binary_op(
        BinaryOp::Or,
        &OracleValue::null_boolean(),
    );
    assert_eq!(false_or_null, OracleValue::null_boolean());
}

#[test]
fn test_conf_03_tier1_builtins_and_dynamic_access() {
    let key = ModuleKey::new("CONF_03_QUERY_CASCADE", ModuleType::Form);
    let sample = resolve_sample("conf_03_query_cascade_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_03_cache_builtins");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).unwrap();

    let mut session = FormSession::new(module);
    session.start().unwrap();

    // 1. Dynamic access via NAME_IN and COPY
    session.set_bind_value("ORDERS.ORDER_STATUS", OracleValue::varchar2("SHIPPED"));
    assert_eq!(
        session.get_bind_value("ORDERS.ORDER_STATUS"),
        OracleValue::varchar2("SHIPPED")
    );

    // 2. Global variables and ERASE
    session.set_bind_value("GLOBAL.USER_ROLE", OracleValue::varchar2("ADMIN"));
    assert_eq!(
        session.get_bind_value("GLOBAL.USER_ROLE"),
        OracleValue::varchar2("ADMIN")
    );
    session.erase_global("GLOBAL.USER_ROLE");
    assert_eq!(
        session.get_bind_value("GLOBAL.USER_ROLE"),
        OracleValue::Null
    );

    // 3. Item properties: SET_ITEM_PROPERTY and GET_ITEM_PROPERTY
    session.set_item_property("ORDERS.ORDER_STATUS", "ENABLED", OracleValue::Number(4.0));
    assert_eq!(
        session.get_item_property("ORDERS.ORDER_STATUS", "ENABLED"),
        OracleValue::Number(4.0)
    );
}

#[test]
fn test_conf_03_step_budget_guardrail() {
    let key = ModuleKey::new("CONF_03_QUERY_CASCADE", ModuleType::Form);
    let sample = resolve_sample("conf_03_query_cascade_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_03_cache_guard");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).unwrap();

    let mut session = FormSession::new(module);
    session.start().unwrap();

    // Infinite loop statement: LOOP NULL; END LOOP;
    let infinite_loop = Statement::Loop {
        body: vec![Statement::Null],
    };

    let mut interpreter = Interpreter::new(false);
    interpreter.max_steps = 100; // Constrain step budget to 100 steps

    let res = interpreter.execute_statement(&infinite_loop, &mut session);
    assert!(matches!(res, Err(RuntimeError::StepBudgetExceeded)));
}
