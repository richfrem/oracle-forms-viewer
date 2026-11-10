use oracle_forms_core::model::{ModuleKey, ModuleType};
use oracle_forms_db::{BoundSql, DatabaseDriver, SqliteDriver};

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
}
use oracle_forms_plsql::ast::{BinaryOp, PlsqlUnit, Statement};
use oracle_forms_runtime::{
    ALERT_BUTTON1, FormSession, InteractionRequest, InteractionResponse, Interpreter, OracleValue,
    RuntimeError,
};
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

    // A real (in-memory SQLite) database holding the ORDERS rows the block queries.
    let db = std::sync::Arc::new(SqliteDriver::in_memory().unwrap());
    block_on(async {
        db.execute(&BoundSql::new("DROP TABLE IF EXISTS orders")).await.unwrap();
        db.execute(&BoundSql::new(
            "CREATE TABLE orders (order_id INTEGER, order_status TEXT, quantity INTEGER, unit_price REAL, total_amount REAL)",
        ))
        .await
        .unwrap();
        db.execute(&BoundSql::new(
            "INSERT INTO orders VALUES (1001, 'OPEN', 5, 20.0, NULL), (1002, 'OPEN', 3, 50.0, NULL)",
        ))
        .await
        .unwrap();
    });
    session.set_database_driver(db);

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
        Some(&serde_json::json!(100))
    );

    // 3. Verify POST-QUERY calculated TOTAL_AMOUNT for record 2 (3 * 50 = 150)
    assert_eq!(
        buf.records[1].get_value("TOTAL_AMOUNT"),
        Some(&serde_json::json!(150))
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

    // The user answers the alert with button 1 (OK). The handler sees the request
    // while the trigger is paused at SHOW_ALERT, and execution then continues.
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_in_handler = seen.clone();
    session.set_interaction_handler(Box::new(move |req: &InteractionRequest| {
        seen_in_handler.lock().unwrap().push(req.clone());
        InteractionResponse::AlertButton(ALERT_BUTTON1)
    }));
    session.set_bind_value("ORDERS.ORDER_STATUS", OracleValue::varchar2("OPEN"));

    let trigger_res = session.dispatch_event("WHEN-BUTTON-PRESSED", Some("BTN_DELETE"), Some("ORDERS"));
    assert!(trigger_res.is_success(), "{:?} / {:?}", trigger_res, session.trace_log);

    let requests = seen.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    match &requests[0] {
        InteractionRequest::ShowAlert { alert_name, message, button1, .. } => {
            assert_eq!(alert_name, "CONFIRM_DELETE");
            assert_eq!(message, "Are you sure you want to cancel this order?");
            assert_eq!(button1, "OK");
        }
    }

    // The IF v_button = ALERT_BUTTON1 branch ran after the answer.
    assert_eq!(session.get_bind_value("ORDERS.ORDER_STATUS"), OracleValue::varchar2("CANCELLED"));
    assert_eq!(session.messages, vec!["Order cancelled."]);
}

#[test]
fn test_conf_03_alert_answered_with_button2_skips_branch() {
    let key = ModuleKey::new("CONF_03_QUERY_CASCADE", ModuleType::Form);
    let sample = resolve_sample("conf_03_query_cascade_fmb.xml");
    let cache_dir = std::env::temp_dir().join("conf_03_cache_alert2");
    let module = FormsXmlParser::parse(&key, &sample, &cache_dir).unwrap();
    let mut session = FormSession::new(module);
    session.start().unwrap();
    session.set_interaction_handler(Box::new(|_req: &InteractionRequest| InteractionResponse::AlertButton(oracle_forms_runtime::ALERT_BUTTON2)));
    session.set_bind_value("ORDERS.ORDER_STATUS", OracleValue::varchar2("OPEN"));
    assert!(session.dispatch_event("WHEN-BUTTON-PRESSED", Some("BTN_DELETE"), Some("ORDERS")).is_success());
    assert_eq!(session.get_bind_value("ORDERS.ORDER_STATUS"), OracleValue::varchar2("OPEN"));
    assert!(session.messages.is_empty());
}

#[test]
fn test_conf_03_oracle_value_semantics_and_three_valued_logic() {
    // 1. Invariant: empty string is strictly NULL
    let empty_v = OracleValue::varchar2("");
    assert_eq!(empty_v, OracleValue::Null);
    assert!(empty_v.is_null());

    // 2. Three-valued NULL equality: NULL = NULL evaluates to NULL (not TRUE!)
    let eq_res = OracleValue::Null.eval_binary_op(BinaryOp::Equal, &OracleValue::Null).unwrap();
    assert_eq!(eq_res, OracleValue::null_boolean());
    assert!(!eq_res.is_truthy());

    // 3. Concatenation: in Oracle, NULL || 'world' is 'world'
    let concat_res = OracleValue::Null.eval_binary_op(
        BinaryOp::Concat,
        &OracleValue::varchar2("world"),
    ).unwrap();
    assert_eq!(concat_res, OracleValue::varchar2("world"));

    // 4. Boolean 3-valued AND: false AND NULL is false; true AND NULL is NULL
    let false_and_null = OracleValue::boolean(false).eval_binary_op(
        BinaryOp::And,
        &OracleValue::null_boolean(),
    ).unwrap();
    assert_eq!(false_and_null, OracleValue::boolean(false));

    let true_and_null = OracleValue::boolean(true).eval_binary_op(
        BinaryOp::And,
        &OracleValue::null_boolean(),
    ).unwrap();
    assert_eq!(true_and_null, OracleValue::null_boolean());

    // 5. Boolean 3-valued OR: true OR NULL is true; false OR NULL is NULL
    let true_or_null = OracleValue::boolean(true).eval_binary_op(
        BinaryOp::Or,
        &OracleValue::null_boolean(),
    ).unwrap();
    assert_eq!(true_or_null, OracleValue::boolean(true));

    let false_or_null = OracleValue::boolean(false).eval_binary_op(
        BinaryOp::Or,
        &OracleValue::null_boolean(),
    ).unwrap();
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
    session.set_item_property("ORDERS.ORDER_STATUS", "ENABLED", OracleValue::int(4));
    assert_eq!(
        session.get_item_property("ORDERS.ORDER_STATUS", "ENABLED"),
        OracleValue::int(4)
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

    let mut interpreter = Interpreter::new();
    interpreter.max_steps = 100; // Constrain step budget to 100 steps

    let res = interpreter.execute_unit(&PlsqlUnit::StatementList(vec![infinite_loop]), &mut session);
    assert!(matches!(res, Err(RuntimeError::StepBudgetExceeded)));
}
