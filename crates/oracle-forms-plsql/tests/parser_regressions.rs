//! Regression tests for parser bugs found in review.
use oracle_forms_plsql::ast::*;
use oracle_forms_plsql::Parser;

fn stmts(src: &str) -> Vec<Statement> {
    match Parser::new(src).parse_unit().expect("parses") {
        PlsqlUnit::StatementList(s) => s,
        PlsqlUnit::AnonymousBlock(b) => b.body,
        other => panic!("unexpected unit {other:?}"),
    }
}

#[test]
fn is_null_is_a_two_valued_test_not_equality_with_null() {
    let s = stmts("IF :b.x IS NULL THEN NULL; END IF; IF :b.y IS NOT NULL THEN NULL; END IF;");
    match &s[0] {
        Statement::If { condition: Expr::UnaryOp { op: UnaryOp::IsNull, .. }, .. } => {}
        other => panic!("IS NULL parsed as {other:?}"),
    }
    match &s[1] {
        Statement::If { condition: Expr::UnaryOp { op: UnaryOp::IsNotNull, .. }, .. } => {}
        other => panic!("IS NOT NULL parsed as {other:?}"),
    }
}

#[test]
fn select_into_keeps_exact_sql_text_including_quotes() {
    let s = stmts("SELECT ename, sal INTO :emp.ename, v_sal FROM emp WHERE job = 'CLERK' AND deptno = :emp.deptno;");
    match &s[0] {
        Statement::Sql(SqlStatement::SelectInto { raw_sql, into_targets, .. }) => {
            assert_eq!(raw_sql, "SELECT ename, sal FROM emp WHERE job = 'CLERK' AND deptno = :emp.deptno");
            assert_eq!(into_targets.len(), 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn dml_and_transaction_control_statements_are_recognised() {
    let s = stmts("UPDATE emp SET sal = sal * 1.1 WHERE ename = 'KING'; DELETE FROM t WHERE id = :b.id; INSERT INTO t (a) VALUES ('x'); COMMIT;");
    let kinds: Vec<_> = s
        .iter()
        .map(|st| match st {
            Statement::Sql(SqlStatement::Dml { kind, raw_sql }) => (*kind, raw_sql.clone()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(kinds[0], (DmlKind::Update, "UPDATE emp SET sal = sal * 1.1 WHERE ename = 'KING'".to_string()));
    assert_eq!(kinds[1].0, DmlKind::Delete);
    assert_eq!(kinds[2], (DmlKind::Insert, "INSERT INTO t (a) VALUES ('x')".to_string()));
    assert_eq!(kinds[3].0, DmlKind::Commit);
}

#[test]
fn builtin_calls_named_like_sql_keywords_stay_calls() {
    let s = stmts("COMMIT_FORM; update_totals(1);");
    assert!(matches!(&s[0], Statement::Call { target, .. } if target == "COMMIT_FORM"));
    assert!(matches!(&s[1], Statement::Call { target, .. } if target == "update_totals"));
}

#[test]
fn exit_when_in_loop() {
    let s = stmts("LOOP v := v + 1; EXIT WHEN v > 3; END LOOP;");
    match &s[0] {
        Statement::Loop { body } => assert!(matches!(body[1], Statement::Exit { condition: Some(_) })),
        other => panic!("{other:?}"),
    }
}

#[test]
fn out_and_in_out_parameter_modes() {
    let unit = Parser::new("PROCEDURE p(a IN NUMBER, b OUT VARCHAR2, c IN OUT NOCOPY NUMBER) IS BEGIN NULL; END;")
        .parse_unit()
        .unwrap();
    match unit {
        PlsqlUnit::Procedure(p) => {
            let modes: Vec<_> = p.params.iter().map(|x| x.mode).collect();
            assert_eq!(modes, vec![ParamMode::In, ParamMode::Out, ParamMode::InOut]);
            assert_eq!(p.params[1].data_type, DataType::Primitive("VARCHAR2".into()));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn package_function_call_has_dotted_name() {
    let s = stmts("v := my_pkg.get_total(:b.id);");
    match &s[0] {
        Statement::Assignment { value: Expr::FunctionCall { name, .. }, .. } => assert_eq!(name, "my_pkg.get_total"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn in_list_between_not_like_and_attributes() {
    let s = stmts("IF x IN (1, 2) AND y NOT BETWEEN 1 AND 5 AND z NOT LIKE 'A%' AND SQL%ROWCOUNT = 0 THEN NULL; END IF;");
    let Statement::If { condition, .. } = &s[0] else { panic!() };
    let dbg = format!("{condition:?}");
    assert!(dbg.contains("InList"), "{dbg}");
    assert!(dbg.contains("Between") && dbg.contains("negated: true"), "{dbg}");
    assert!(dbg.contains("Like"), "{dbg}");
    assert!(dbg.contains("Attribute { object: \"SQL\", attr: \"ROWCOUNT\" }"), "{dbg}");
}

#[test]
fn cursors_open_fetch_close_and_cursor_for_loops() {
    let src = "DECLARE CURSOR c1(p_dept NUMBER) IS SELECT ename FROM emp WHERE deptno = p_dept; v VARCHAR2(30);
               BEGIN OPEN c1(10); FETCH c1 INTO v; CLOSE c1;
               FOR r IN c1(20) LOOP NULL; END LOOP;
               FOR r IN (SELECT 'x' AS a FROM dual) LOOP NULL; END LOOP; END;";
    let unit = Parser::new(src).parse_unit().unwrap();
    let PlsqlUnit::AnonymousBlock(b) = unit else { panic!() };
    match &b.declarations[0] {
        Declaration::Cursor(c) => {
            assert_eq!(c.params.len(), 1);
            assert_eq!(c.query, "SELECT ename FROM emp WHERE deptno = p_dept");
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(&b.body[0], Statement::OpenCursor { cursor, args } if cursor == "c1" && args.len() == 1));
    assert!(matches!(&b.body[1], Statement::FetchCursor { .. }));
    assert!(matches!(&b.body[2], Statement::CloseCursor { .. }));
    assert!(matches!(&b.body[3], Statement::CursorForLoop { cursor: Some(c), .. } if c == "c1"));
    assert!(matches!(&b.body[4], Statement::CursorForLoop { query: Some(q), .. } if q == "SELECT 'x' AS a FROM dual"));
}

#[test]
fn case_statement_and_expression_and_named_args() {
    let s = stmts("CASE v WHEN 1 THEN x := 'a'; ELSE x := 'b'; END CASE; y := CASE WHEN v > 1 THEN 'big' ELSE 'small' END; p(a => 1, b => 2);");
    assert!(matches!(&s[0], Statement::Case { selector: Some(_), .. }));
    assert!(matches!(&s[1], Statement::Assignment { value: Expr::Case { .. }, .. }));
    assert!(matches!(&s[2], Statement::Call { args, .. } if matches!(args[0], Expr::NamedArg { .. })));
}
