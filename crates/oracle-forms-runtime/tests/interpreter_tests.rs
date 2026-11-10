//! Interpreter behaviour against real Forms PL/SQL: program units, attached libraries,
//! embedded SQL (bound, against SQLite), cursors, exceptions and built-in semantics.

use oracle_forms_core::model::{ModuleKey, ModuleType, ProgramUnitInfo, ProgramUnitType, TriggerInfo, TriggerLevel};
use oracle_forms_db::{BoundSql, DatabaseDriver, SqliteDriver};
use oracle_forms_runtime::{FormSession, OracleValue, TriggerResult};
use oracle_forms_xml::{FormsXmlParser, PldParser};
use std::path::PathBuf;
use std::sync::Arc;

fn sample(name: &str) -> PathBuf {
    for p in ["", "..", "../.."] {
        for d in ["samples", "samples/conformance"] {
            let c = PathBuf::from(p).join(d).join(name);
            if c.is_file() {
                return c;
            }
        }
    }
    panic!("sample {name} not found");
}

fn unit(name: &str, kind: ProgramUnitType, source: &str) -> ProgramUnitInfo {
    ProgramUnitInfo {
        name: name.into(),
        unit_type: kind,
        line_count: source.lines().count(),
        first_line: source.lines().next().unwrap_or("").into(),
        inherited: None,
        text_encoding: Default::default(),
        text_ref: None,
        xml_ref: None,
        source_text: Some(source.into()),
    }
}

fn trigger(name: &str, block: Option<&str>, item: Option<&str>, source: &str) -> TriggerInfo {
    TriggerInfo {
        name: name.into(),
        level: match (block, item) {
            (Some(_), Some(_)) => TriggerLevel::Item,
            (Some(_), None) => TriggerLevel::Block,
            _ => TriggerLevel::Form,
        },
        block_name: block.map(Into::into),
        item_name: item.map(Into::into),
        first_line: source.lines().next().unwrap_or("").into(),
        line_count: source.lines().count(),
        inherited: None,
        text_encoding: Default::default(),
        text_ref: None,
        xml_ref: None,
        source_text: Some(source.into()),
        execution_hierarchy: None,
    }
}

/// ORDERS form (from conf_03) with extra program units / triggers, attached to an
/// in-memory SQLite database.
fn session_with(units: Vec<ProgramUnitInfo>, triggers: Vec<TriggerInfo>) -> (FormSession, Arc<SqliteDriver>) {
    let key = ModuleKey::new("INTERP", ModuleType::Form);
    let cache = std::env::temp_dir().join(format!("ofw-interp-{}", std::process::id()));
    let mut module = FormsXmlParser::parse(&key, &sample("conf_03_query_cascade_fmb.xml"), &cache).unwrap();
    module.program_units = units;
    module.triggers.retain(|t| t.name != "WHEN-BUTTON-PRESSED");
    module.triggers.extend(triggers);
    let db = Arc::new(SqliteDriver::in_memory().unwrap());
    let mut session = FormSession::new(module);
    session.set_database_driver(db.clone());
    (session, db)
}

fn press(session: &mut FormSession) -> TriggerResult {
    session.dispatch_event("WHEN-BUTTON-PRESSED", Some("BTN_DELETE"), Some("ORDERS"))
}

fn button(source: &str) -> TriggerInfo {
    trigger("WHEN-BUTTON-PRESSED", Some("ORDERS"), Some("BTN_DELETE"), source)
}

fn item(session: &FormSession, name: &str) -> OracleValue {
    session.get_bind_value(&format!("ORDERS.{}", name))
}

#[test]
fn trigger_calls_form_procedure_with_out_parameter_and_function() {
    let (mut s, _) = session_with(
        vec![
            unit(
                "SET_STATUS",
                ProgramUnitType::Procedure,
                "PROCEDURE set_status(p_qty IN NUMBER, p_status OUT VARCHAR2) IS\nBEGIN\n  IF p_qty > 10 THEN p_status := 'BULK'; ELSE p_status := 'SMALL'; END IF;\nEND;",
            ),
            unit("DOUBLE_IT", ProgramUnitType::Function, "FUNCTION double_it(n NUMBER) RETURN NUMBER IS BEGIN RETURN n * 2; END;"),
        ],
        vec![button("DECLARE v VARCHAR2(10); BEGIN set_status(:ORDERS.QUANTITY, v); :ORDERS.ORDER_STATUS := v; :ORDERS.TOTAL_AMOUNT := double_it(:ORDERS.QUANTITY); END;")],
    );
    s.set_bind_value("ORDERS.QUANTITY", OracleValue::int(12));
    assert_eq!(press(&mut s), TriggerResult::Success, "{:?}", s.trace_log);
    assert_eq!(item(&s, "ORDER_STATUS"), OracleValue::varchar2("BULK"));
    assert_eq!(item(&s, "TOTAL_AMOUNT"), OracleValue::int(24));
}

#[test]
fn attached_pld_library_units_are_callable() {
    let (mut s, _) = session_with(vec![], vec![button("BEGIN :ORDERS.TOTAL_AMOUNT := add_numbers(2, 3); pkg_util.noop; END;")]);
    let lib = PldParser::parse(&ModuleKey::new("UTILS", ModuleType::Library), &sample("utils.pld"), &std::env::temp_dir().join("ofw-pld")).unwrap();
    s.attach_library(&lib);
    assert_eq!(press(&mut s), TriggerResult::Success, "{:?}", s.trace_log);
    assert_eq!(item(&s, "TOTAL_AMOUNT"), OracleValue::int(5));
}

#[test]
fn unknown_call_fails_the_trigger_instead_of_doing_nothing() {
    let (mut s, _) = session_with(vec![], vec![button("BEGIN no_such_procedure(1); :ORDERS.ORDER_STATUS := 'RAN'; END;")]);
    assert!(!press(&mut s).is_success());
    assert_ne!(item(&s, "ORDER_STATUS"), OracleValue::varchar2("RAN"));
    assert!(s.messages.iter().any(|m| m.contains("NO_SUCH_PROCEDURE")), "{:?}", s.messages);
}

#[test]
fn select_into_binds_items_and_locals_and_maps_columns_by_position() {
    let (mut s, _) = session_with(
        vec![],
        vec![button(
            "DECLARE v_id NUMBER := 3; BEGIN SELECT amount, item_name INTO :ORDERS.TOTAL_AMOUNT, :ORDERS.ORDER_STATUS FROM test_items WHERE item_id = v_id AND item_name <> 'it''s'; END;",
        )],
    );
    assert_eq!(press(&mut s), TriggerResult::Success, "{:?}", s.trace_log);
    // test_items row 3 = ('Gizmo C', 25.00): columns land in INTO order, not hash order.
    assert_eq!(item(&s, "ORDER_STATUS"), OracleValue::varchar2("Gizmo C"));
    assert_eq!(item(&s, "TOTAL_AMOUNT"), OracleValue::int(25));
    let sql = s.trace_log.iter().find(|t| t.starts_with("SQL: ")).unwrap();
    assert!(sql.contains("item_id = ?1") && !sql.contains("v_id"), "local variable not bound: {sql}");
}

#[test]
fn no_data_found_and_too_many_rows_are_catchable() {
    let (mut s, _) = session_with(
        vec![],
        vec![button(
            "DECLARE v VARCHAR2(30); BEGIN
               BEGIN SELECT item_name INTO v FROM test_items WHERE item_id = -1;
               EXCEPTION WHEN NO_DATA_FOUND THEN :ORDERS.ORDER_STATUS := 'NONE'; END;
               BEGIN SELECT item_name INTO v FROM test_items;
               EXCEPTION WHEN TOO_MANY_ROWS THEN :ORDERS.TOTAL_AMOUNT := SQL%ROWCOUNT; END;
             END;",
        )],
    );
    assert_eq!(press(&mut s), TriggerResult::Success, "{:?}", s.trace_log);
    assert_eq!(item(&s, "ORDER_STATUS"), OracleValue::varchar2("NONE"));
    assert_eq!(item(&s, "TOTAL_AMOUNT"), OracleValue::int(5));
}

#[test]
fn sqlerrm_reports_the_database_error() {
    let (mut s, _) = session_with(
        vec![],
        vec![button("DECLARE v NUMBER; BEGIN SELECT no_such_col INTO v FROM test_items WHERE item_id = 1; EXCEPTION WHEN OTHERS THEN :ORDERS.ORDER_STATUS := SQLERRM; END;")],
    );
    assert_eq!(press(&mut s), TriggerResult::Success);
    let msg = item(&s, "ORDER_STATUS").to_string_lossy();
    assert!(msg.to_ascii_lowercase().contains("no_such_col"), "{msg}");
}

#[test]
fn explicit_cursor_loop_and_cursor_for_loop() {
    let (mut s, _) = session_with(
        vec![],
        vec![button(
            "DECLARE
               CURSOR c(p_min NUMBER) IS SELECT amount FROM test_items WHERE amount >= p_min ORDER BY item_id;
               v NUMBER; total NUMBER := 0; n NUMBER := 0;
             BEGIN
               OPEN c(20);
               LOOP FETCH c INTO v; EXIT WHEN c%NOTFOUND; total := total + v; END LOOP;
               CLOSE c;
               FOR r IN (SELECT item_id FROM test_items) LOOP n := n + 1; END LOOP;
               :ORDERS.TOTAL_AMOUNT := total; :ORDERS.QUANTITY := n;
             END;",
        )],
    );
    assert_eq!(press(&mut s), TriggerResult::Success, "{:?}", s.trace_log);
    // amounts >= 20: 99.50 + 25.00 + 150.00
    assert_eq!(item(&s, "TOTAL_AMOUNT"), OracleValue::number_from_str("274.5"));
    assert_eq!(item(&s, "QUANTITY"), OracleValue::int(5));
}

#[test]
fn is_null_branches_and_form_trigger_failure_handling() {
    let (mut s, _) = session_with(
        vec![],
        vec![button(
            "BEGIN
               IF :ORDERS.ORDER_ID IS NULL THEN :ORDERS.ORDER_STATUS := 'NO ID'; END IF;
               BEGIN RAISE FORM_TRIGGER_FAILURE; EXCEPTION WHEN FORM_TRIGGER_FAILURE THEN :ORDERS.QUANTITY := 1; END;
               RAISE FORM_TRIGGER_FAILURE;
             END;",
        )],
    );
    let res = press(&mut s);
    assert!(matches!(res, TriggerResult::Failure(_)), "{res:?}");
    assert_eq!(item(&s, "ORDER_STATUS"), OracleValue::varchar2("NO ID"));
    assert_eq!(item(&s, "QUANTITY"), OracleValue::int(1));
}

#[test]
fn failed_builtin_sets_form_failure_and_continues() {
    let (mut s, _) = session_with(
        vec![],
        vec![button("BEGIN GO_ITEM('ORDERS.NOPE'); IF FORM_FAILURE THEN :ORDERS.ORDER_STATUS := 'NAV FAILED'; END IF; END;")],
    );
    assert_eq!(press(&mut s), TriggerResult::Success, "{:?}", s.trace_log);
    assert_eq!(item(&s, "ORDER_STATUS"), OracleValue::varchar2("NAV FAILED"));
}

#[test]
fn package_state_persists_across_calls() {
    let (mut s, _) = session_with(
        vec![
            unit("COUNTER", ProgramUnitType::PackageSpec, "PACKAGE counter IS n NUMBER := 0; PROCEDURE bump; END;"),
            unit("COUNTER", ProgramUnitType::PackageBody, "PACKAGE BODY counter IS PROCEDURE bump IS BEGIN n := n + 1; END; END;"),
        ],
        vec![button("BEGIN counter.bump; counter.bump; :ORDERS.QUANTITY := counter.n; END;")],
    );
    assert_eq!(press(&mut s), TriggerResult::Success, "{:?}", s.trace_log);
    assert_eq!(item(&s, "QUANTITY"), OracleValue::int(2));
    assert_eq!(press(&mut s), TriggerResult::Success);
    assert_eq!(item(&s, "QUANTITY"), OracleValue::int(4));
}

#[test]
fn unparseable_trigger_is_reported_not_skipped() {
    let (mut s, _) = session_with(vec![], vec![button("BEGIN this is not plsql at all ;; END;")]);
    assert!(!press(&mut s).is_success());
    assert!(s.trace_log.iter().any(|t| t.starts_with("TRIGGER_PARSE_ERROR") || t.starts_with("RUNTIME_ERROR")), "{:?}", s.trace_log);
}

#[test]
fn dml_in_trigger_runs_and_is_blocked_in_read_only_mode() {
    let (mut s, db) = session_with(
        vec![],
        vec![button("BEGIN UPDATE test_items SET amount = amount + 1 WHERE item_id = 1; :ORDERS.QUANTITY := SQL%ROWCOUNT; END;")],
    );
    assert_eq!(press(&mut s), TriggerResult::Success, "{:?}", s.trace_log);
    assert_eq!(item(&s, "QUANTITY"), OracleValue::int(1));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let r = rt.block_on(db.query(&BoundSql::new("SELECT amount FROM test_items WHERE item_id = 1"))).unwrap();
    assert_eq!(r.records[0].values["AMOUNT"], serde_json::json!(11.5));

    s.read_only = true;
    let res = press(&mut s);
    assert!(matches!(res, TriggerResult::UnhandledException(ref m) if m.contains("READ_ONLY")), "{res:?}");
}

#[test]
fn execution_hierarchy_is_read_per_trigger() {
    let mut item_trigger = trigger("WHEN-NEW-ITEM-INSTANCE", Some("ORDERS"), Some("ORDER_ID"), "BEGIN :GLOBAL.ORDER := :GLOBAL.ORDER || 'I'; END;");
    item_trigger.execution_hierarchy = Some("After".into());
    let block_trigger = trigger("WHEN-NEW-ITEM-INSTANCE", Some("ORDERS"), None, "BEGIN :GLOBAL.ORDER := :GLOBAL.ORDER || 'B'; END;");
    let form_trigger = trigger("WHEN-NEW-ITEM-INSTANCE", None, None, "BEGIN :GLOBAL.ORDER := :GLOBAL.ORDER || 'F'; END;");
    let (mut s, _) = session_with(vec![], vec![item_trigger, block_trigger, form_trigger]);
    s.dispatch_event("WHEN-NEW-ITEM-INSTANCE", Some("ORDER_ID"), Some("ORDERS"));
    // Item trigger says AFTER: item runs, then the block trigger; the block trigger has the
    // default OVERRIDE, so the form-level one does not run.
    assert_eq!(s.get_bind_value("GLOBAL.ORDER"), OracleValue::varchar2("IB"));
}

#[test]
fn decimal_arithmetic_in_triggers_is_exact() {
    let (mut s, _) = session_with(vec![], vec![button("BEGIN :ORDERS.TOTAL_AMOUNT := 0.1 + 0.2; IF 0.1 + 0.2 = 0.3 THEN :ORDERS.ORDER_STATUS := 'EXACT'; END IF; END;")]);
    assert_eq!(press(&mut s), TriggerResult::Success);
    assert_eq!(item(&s, "ORDER_STATUS"), OracleValue::varchar2("EXACT"));
}
