use std::collections::HashMap;
use std::path::PathBuf;
use oracle_forms_db::mock::MockDriver;
use oracle_forms_runtime::{ReportDataEvaluator, ReportRenderer};
use oracle_forms_xml::ReportsXmlParser;

fn samples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("samples")
}

#[tokio::test]
async fn test_orders_summary_report_parsing_and_evaluation() {
    let report_file = samples_dir().join("orders_summary_report.xml");
    assert!(report_file.exists(), "Sample report file missing: {:?}", report_file);

    // 1. Parse Reports XML
    let report = ReportsXmlParser::parse_file(&report_file).expect("Failed to parse orders_summary_report.xml");
    assert_eq!(report.name, "ORDERS_SUMMARY");
    assert_eq!(report.title.as_deref(), Some("Customer Orders Summary Report"));
    assert_eq!(report.data.data_sources.len(), 1);
    assert_eq!(report.data.data_sources[0].groups.len(), 1);

    let grp = &report.data.data_sources[0].groups[0];
    assert_eq!(grp.name, "G_ORDERS");
    assert_eq!(grp.summaries.len(), 3);

    // 2. Evaluate Data Model against Mock / SQLite Driver
    let driver = MockDriver::new().expect("Failed to init MockDriver");
    let result = ReportDataEvaluator::evaluate(&report, &driver, &HashMap::new())
        .await
        .expect("Failed to evaluate report");

    assert_eq!(result.report_name, "ORDERS_SUMMARY");
    assert_eq!(result.root_groups, vec!["G_ORDERS"]);

    let orders_recs = result.group_records.get("G_ORDERS").expect("G_ORDERS records missing");
    assert_eq!(orders_recs.len(), 4, "Expected 4 seeded orders");

    // Check individual record values
    let first = &orders_recs[0];
    assert_eq!(first.values.get("ORDER_ID").map(|s| s.as_str()), Some("1001"));
    assert!(first.values.contains_key("ORDER_TOTAL"));

    // Check Report-level Summaries
    assert_eq!(result.report_summaries.get("CS_ORDER_COUNT").copied(), Some(4.0));

    let total_sales = result.report_summaries.get("CS_TOTAL_SALES").copied().unwrap_or(0.0);
    // 1250.00 + 4300.50 + 890.00 + 620.25 = 7060.75
    assert!((total_sales - 7060.75).abs() < 0.01, "Expected total sales ~7060.75, got {}", total_sales);

    let avg_order = result.report_summaries.get("CS_AVG_ORDER").copied().unwrap_or(0.0);
    assert!((avg_order - (7060.75 / 4.0)).abs() < 0.01);

    // 3. Render HTML5 Paged Media Output
    let html = ReportRenderer::render_html(&report, &result);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("Customer Orders Summary Report"));
    assert!(html.contains("<table class=\"report-table\" data-group=\"G_ORDERS\">"));
    assert!(html.contains("1001"));
    assert!(html.contains("$1,250.00"));
    assert!(html.contains("$7,060.75"));
    assert!(html.contains("@page {"));
    assert!(html.contains("window.print()"));
}

#[tokio::test]
async fn test_department_salaries_master_detail_report() {
    let report_file = samples_dir().join("department_salaries_report.xml");
    assert!(report_file.exists(), "Sample report file missing: {:?}", report_file);

    // 1. Parse Master-Detail Reports XML
    let report = ReportsXmlParser::parse_file(&report_file).expect("Failed to parse department_salaries_report.xml");
    assert_eq!(report.name, "DEPT_SALARIES");
    assert_eq!(report.data.data_sources.len(), 2);
    assert_eq!(report.data.links.len(), 1);

    let link = &report.data.links[0];
    assert_eq!(link.parent_group, "G_DEPT");
    assert_eq!(link.child_query, "Q_EMPLOYEES");

    // 2. Evaluate Master-Detail Data Model
    let driver = MockDriver::new().expect("Failed to init MockDriver");
    let result = ReportDataEvaluator::evaluate(&report, &driver, &HashMap::new())
        .await
        .expect("Failed to evaluate master-detail report");

    assert_eq!(result.root_groups, vec!["G_DEPT"]);

    let dept_recs = result.group_records.get("G_DEPT").expect("G_DEPT records missing");
    assert_eq!(dept_recs.len(), 4, "Expected 4 seeded departments (10, 20, 30, 60)");

    // Check Department 10 (Administration)
    let dept10 = dept_recs.iter().find(|d| d.values.get("DEPARTMENT_ID").map(|s| s.as_str()) == Some("10")).expect("Dept 10 not found");
    assert_eq!(dept10.values.get("DEPARTMENT_NAME").map(|s| s.as_str()), Some("Administration"));

    let dept10_emps = dept10.child_records.get("G_EMP").expect("Dept 10 G_EMP children missing");
    assert_eq!(dept10_emps.len(), 3, "Expected 3 employees in Dept 10 (Steven, Neena, Lex)");

    // Verify formula CF_BONUS (10% of salary: Steven King 24000 -> 2400.00)
    let steven = dept10_emps.iter().find(|e| e.values.get("FIRST_NAME").map(|s| s.as_str()) == Some("Steven")).expect("Steven not found");
    assert_eq!(steven.values.get("CF_BONUS").map(|s| s.as_str()), Some("2400.00"));

    // Verify group summaries: Department 10 total salary = 24000 + 17000 + 17000 = 58000
    let dept10_sum = dept10.group_summaries.get("CS_DEPT_TOTAL_SALARY").copied().unwrap_or(0.0);
    assert_eq!(dept10_sum, 58000.0);
    assert_eq!(dept10.group_summaries.get("CS_DEPT_EMP_COUNT").copied(), Some(3.0));

    // Verify Report Grand Total:
    // Total payroll = (24k + 17k + 17k) + (13k + 6k) + (11k) + (9k + 6k) = 58000 + 19000 + 11000 + 15000 = 103000
    let total_payroll = result.report_summaries.get("CS_TOTAL_PAYROLL").copied().unwrap_or(0.0);
    assert_eq!(total_payroll, 103000.0);

    // 3. Render HTML
    let html = ReportRenderer::render_html(&report, &result);
    assert!(html.contains("Department Payroll &amp; Compensation Report"));
    assert!(html.contains("master-record-card"));
    assert!(html.contains("Administration"));
    assert!(html.contains("Steven"));
    assert!(html.contains("$24,000.00"));
    assert!(html.contains("$2,400.00"));
    assert!(html.contains("$58,000.00"));
    assert!(html.contains("$103,000.00"));
}

// SQLite executes the binds, so these assertions detect ignored parameters and interpolation.
#[tokio::test]
async fn report_parameters_bind_defaults_overrides_nulls_and_literal_text() {
    use oracle_forms_core::report::ReportParameter;
    use oracle_forms_db::sqlite::SqliteDriver;
    let mut report = ReportsXmlParser::parse_file(&samples_dir().join("orders_summary_report.xml")).unwrap();
    report.data.data_sources[0].select_sql =
        "SELECT :P_TEXT AS LABEL, :P_NUM + 1 AS TOTAL, ':NOT_A_BIND' AS LITERAL, :P_TEXT AS AGAIN /* :IGNORED */".into();
    report.data.data_sources[0].groups[0].formulas.clear();
    report.data.data_sources[0].groups[0].summaries.clear();
    report.data.parameters = vec![
        ReportParameter { name: "P_TEXT".into(), datatype: Some("character".into()), initial_value: Some("default".into()) },
        ReportParameter { name: "P_NUM".into(), datatype: Some("number".into()), initial_value: Some("41".into()) },
    ];
    let driver = SqliteDriver::in_memory().unwrap();
    let result = ReportDataEvaluator::evaluate(&report, &driver, &HashMap::new()).await.unwrap();
    let row = &result.group_records["G_ORDERS"][0].values;
    assert_eq!(row["LABEL"], "default");
    assert_eq!(row["TOTAL"], "42");
    assert_eq!(row["LITERAL"], ":NOT_A_BIND");

    let hostile = "O'Reilly'); DROP TABLE orders; --";
    let params = HashMap::from([("p_text".into(), hostile.into()), ("p_num".into(), "5".into())]);
    let result = ReportDataEvaluator::evaluate(&report, &driver, &params).await.unwrap();
    let row = &result.group_records["G_ORDERS"][0].values;
    assert_eq!(row["LABEL"], hostile);
    assert_eq!(row["AGAIN"], hostile);
    assert_eq!(row["TOTAL"], "6");

    let params = HashMap::from([("P_TEXT".into(), String::new())]);
    let result = ReportDataEvaluator::evaluate(&report, &driver, &params).await.unwrap();
    assert_eq!(result.group_records["G_ORDERS"][0].values["LABEL"], "");
    let params = HashMap::from([("P_NUM".into(), "not a number".into())]);
    assert!(ReportDataEvaluator::evaluate(&report, &driver, &params).await.unwrap_err().to_string().contains("must be numeric"));
    report.data.data_sources[0].select_sql = "SELECT :MISSING AS VALUE".into();
    assert!(ReportDataEvaluator::evaluate(&report, &driver, &HashMap::new()).await.unwrap_err().to_string().contains("Missing report parameter :MISSING"));
}
