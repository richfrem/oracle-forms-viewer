use std::collections::HashMap;
use oracle_forms_core::error::{FormsError, Result};
use oracle_forms_core::report::*;
use oracle_forms_db::driver::DatabaseDriver;
use oracle_forms_db::model::{BoundSql, SqlValue};
use oracle_forms_db::generator::rewrite_binds;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportExecutionResult {
    pub report_name: String,
    pub title: String,
    pub unit: String,
    pub root_groups: Vec<String>,
    pub group_records: HashMap<String, Vec<ReportRecord>>,
    pub report_summaries: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRecord {
    pub values: HashMap<String, String>,
    pub child_records: HashMap<String, Vec<ReportRecord>>,
    pub group_summaries: HashMap<String, f64>,
}

pub struct ReportDataEvaluator;

impl ReportDataEvaluator {
    pub async fn evaluate(
        report: &ReportDefinition,
        driver: &dyn DatabaseDriver,
        params: &HashMap<String, String>,
    ) -> Result<ReportExecutionResult> {
        let mut group_records_map: HashMap<String, Vec<ReportRecord>> = HashMap::new();
        let mut query_results: HashMap<String, Vec<HashMap<String, String>>> = HashMap::new();

        // 1. Execute SQL for all data sources
        for ds in &report.data.data_sources {
            let sql = ds.select_sql.trim();
            if sql.is_empty() {
                continue;
            }
            let rewritten = rewrite_binds(sql, driver.backend(), 1);
            let mut binds = Vec::new();
            for reference in &rewritten.refs {
                let definition = report.data.parameters.iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&reference.reference));
                let supplied: Vec<_> = params.iter()
                    .filter(|(name, _)| name.eq_ignore_ascii_case(&reference.reference))
                    .collect();
                if supplied.len() > 1 {
                    return Err(FormsError::Database(format!(
                        "Ambiguous report parameter :{}", reference.reference
                    )));
                }
                let value = supplied.first().map(|(_, value)| value.as_str())
                    .or_else(|| definition.and_then(|p| p.initial_value.as_deref()));
                let value = match value {
                    Some("") => SqlValue::Null,
                    Some(value) => {
                        let datatype = definition.and_then(|p| p.datatype.as_deref())
                            .unwrap_or("character").to_ascii_lowercase();
                        if matches!(datatype.as_str(), "number" | "integer" | "float" | "decimal") {
                            value.trim().parse::<bigdecimal::BigDecimal>().map_err(|_| {
                                FormsError::Database(format!(
                                    "Report parameter :{} must be numeric", reference.reference
                                ))
                            })?;
                            SqlValue::Number(value.trim().to_string())
                        } else {
                            SqlValue::Text(value.to_string())
                        }
                    }
                    None if definition.is_some() => SqlValue::Null,
                    None => return Err(FormsError::Database(format!(
                        "Missing report parameter :{} in query '{}'", reference.reference, ds.name
                    ))),
                };
                binds.push((reference.placeholder.clone(), value));
            }
            let res = driver
                .query(&BoundSql::with_binds(rewritten.sql, binds))
                .await
                .map_err(|e| FormsError::Database(format!("Error executing report query '{}': {}", ds.name, e)))?;

            let rows: Vec<HashMap<String, String>> = res
                .records
                .into_iter()
                .map(|r| {
                    r.values
                        .into_iter()
                        .map(|(k, v)| {
                            let s = match v {
                                serde_json::Value::String(s) => s,
                                serde_json::Value::Null => String::new(),
                                other => other.to_string(),
                            };
                            (k, s)
                        })
                        .collect()
                })
                .collect();
            query_results.insert(ds.name.clone(), rows);
        }

        // 2. Identify parent-child query links
        // link: parent_group -> parent_column -> child_query -> child_column
        let mut child_to_link: HashMap<String, &ReportLink> = HashMap::new();
        for link in &report.data.links {
            child_to_link.insert(link.child_query.to_ascii_uppercase(), link);
        }

        // 3. Populate groups from queries
        // Group query map
        let mut group_to_ds: HashMap<String, &ReportDataSource> = HashMap::new();
        for ds in &report.data.data_sources {
            for grp in &ds.groups {
                group_to_ds.insert(grp.name.to_ascii_uppercase(), ds);
            }
        }

        // Populate records for each data source's groups
        for ds in &report.data.data_sources {
            let rows = query_results.get(&ds.name).cloned().unwrap_or_default();
            for grp in &ds.groups {
                let mut records: Vec<ReportRecord> = Vec::new();
                for row in &rows {
                    let mut values = row.clone();

                    // Evaluate formula columns for this row
                    for formula in &grp.formulas {
                        let computed = evaluate_formula(formula, &values);
                        values.insert(formula.name.clone(), computed);
                    }

                    records.push(ReportRecord {
                        values,
                        child_records: HashMap::new(),
                        group_summaries: HashMap::new(),
                    });
                }
                group_records_map.insert(grp.name.clone(), records);
            }
        }

        // 4. Resolve Master-Detail nesting via Links
        let mut root_groups: Vec<String> = Vec::new();

        for ds in &report.data.data_sources {
            let is_child_query = child_to_link.contains_key(&ds.name.to_ascii_uppercase());
            if !is_child_query {
                for grp in &ds.groups {
                    root_groups.push(grp.name.clone());
                }
            }
        }

        // If no links or all are independent, root groups are all groups
        if root_groups.is_empty() {
            for ds in &report.data.data_sources {
                for grp in &ds.groups {
                    root_groups.push(grp.name.clone());
                }
            }
        }

        // For each link, attach detail records into corresponding parent records
        for link in &report.data.links {
            let parent_grp_name = link.parent_group.clone();
            let parent_col = link.parent_column.to_ascii_uppercase();
            let child_col = link.child_column.to_ascii_uppercase();

            // Find child group name
            let child_ds_name = link.child_query.to_ascii_uppercase();
            let child_ds = report.data.data_sources.iter().find(|d| d.name.eq_ignore_ascii_case(&child_ds_name));
            let child_grp = child_ds.and_then(|d| d.groups.first());

            if let Some(child_g) = child_grp {
                let child_records_all = group_records_map.get(&child_g.name).cloned().unwrap_or_default();
                let child_grp_name = child_g.name.clone();

                if let Some(parent_records) = group_records_map.get_mut(&parent_grp_name) {
                    for parent_rec in parent_records.iter_mut() {
                    let parent_val = parent_rec
                        .values
                        .get(&parent_col)
                        .cloned()
                        .or_else(|| parent_rec.values.get(&link.parent_column).cloned())
                        .unwrap_or_default();

                    let matched_children: Vec<ReportRecord> = child_records_all
                        .iter()
                        .filter(|cr| {
                            let cv = cr
                                .values
                                .get(&child_col)
                                .cloned()
                                .or_else(|| cr.values.get(&link.child_column).cloned())
                                .unwrap_or_default();
                            cv.eq_ignore_ascii_case(&parent_val)
                        })
                        .cloned()
                        .collect();

                    // Evaluate group summaries for this parent record from matched children:
                    // Check summaries defined on parent group resetting at parent_grp_name
                    if let Some(parent_g) = report.data.data_sources.iter().flat_map(|d| &d.groups).find(|g| g.name.eq_ignore_ascii_case(&parent_grp_name)) {
                        for sum in &parent_g.summaries {
                            if let Some(ref r) = sum.reset {
                                if r.eq_ignore_ascii_case(&parent_grp_name) {
                                    let val = calculate_summary(sum, &matched_children);
                                    parent_rec.group_summaries.insert(sum.name.clone(), val);
                                }
                            }
                        }
                    }

                    // Check summaries defined on child group resetting at parent_grp_name or child_grp_name
                    for sum in &child_g.summaries {
                        if let Some(ref r) = sum.reset {
                            if r.eq_ignore_ascii_case(&parent_grp_name) || r.eq_ignore_ascii_case(&child_grp_name) {
                                let val = calculate_summary(sum, &matched_children);
                                parent_rec.group_summaries.insert(sum.name.clone(), val);
                            }
                        }
                    }

                    parent_rec.child_records.insert(child_grp_name.clone(), matched_children);
                }
            }
        }
    }

        // 5. Evaluate Report-level summaries across all groups
        let mut report_summaries: HashMap<String, f64> = HashMap::new();
        for ds in &report.data.data_sources {
            for grp in &ds.groups {
                let recs = group_records_map.get(&grp.name).cloned().unwrap_or_default();
                for sum in &grp.summaries {
                    let is_report_reset = match &sum.reset {
                        Some(r) => r.eq_ignore_ascii_case("report") || r.eq_ignore_ascii_case("page"),
                        None => true,
                    };
                    if is_report_reset {
                        let total = calculate_summary(sum, &recs);
                        report_summaries.insert(sum.name.clone(), total);
                    }
                }
            }
        }

        // 6. Calculate PercentOfTotal summaries
        for ds in &report.data.data_sources {
            for grp in &ds.groups {
                let recs = group_records_map.get(&grp.name).cloned().unwrap_or_default();
                for sum in &grp.summaries {
                    if sum.function == SummaryFunction::PercentOfTotal {
                        let grand_total = report_summaries.get(&sum.source).copied().unwrap_or(1.0);
                        let sub_total = calculate_numeric_sum(&sum.source, &recs);
                        let pct = if grand_total != 0.0 {
                            (sub_total / grand_total) * 100.0
                        } else {
                            0.0
                        };
                        report_summaries.insert(sum.name.clone(), pct);
                    }
                }
            }
        }

        Ok(ReportExecutionResult {
            report_name: report.name.clone(),
            title: report.title.clone().unwrap_or_else(|| report.name.clone()),
            unit: report.unit_of_measurement.clone().unwrap_or_else(|| "inch".to_string()),
            root_groups,
            group_records: group_records_map,
            report_summaries,
        })
    }
}

fn evaluate_formula(formula: &ReportFormula, row: &HashMap<String, String>) -> String {
    if let Some(ref expr) = formula.source {
        let clean = expr.trim();
        // Check for common patterns like ":SALARY * 0.10" or ":PRICE * :QTY"
        if clean.contains('*') {
            let parts: Vec<&str> = clean.split('*').map(|p| p.trim()).collect();
            if parts.len() == 2 {
                let val1 = extract_operand_value(parts[0], row);
                let val2 = extract_operand_value(parts[1], row);
                return format!("{:.2}", val1 * val2);
            }
        } else if clean.contains('+') {
            let parts: Vec<&str> = clean.split('+').map(|p| p.trim()).collect();
            if parts.len() == 2 {
                let val1 = extract_operand_value(parts[0], row);
                let val2 = extract_operand_value(parts[1], row);
                return format!("{:.2}", val1 + val2);
            }
        } else if clean.contains('-') {
            let parts: Vec<&str> = clean.split('-').map(|p| p.trim()).collect();
            if parts.len() == 2 {
                let val1 = extract_operand_value(parts[0], row);
                let val2 = extract_operand_value(parts[1], row);
                return format!("{:.2}", val1 - val2);
            }
        }
    }
    String::new()
}

fn extract_operand_value(op: &str, row: &HashMap<String, String>) -> f64 {
    let stripped = op.trim().trim_start_matches(':');
    if let Ok(n) = stripped.parse::<f64>() {
        n
    } else {
        row.get(stripped)
            .or_else(|| row.get(&stripped.to_ascii_uppercase()))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
    }
}

fn calculate_summary(summary: &ReportSummary, records: &[ReportRecord]) -> f64 {
    let source_col = summary.source.to_ascii_uppercase();

    match summary.function {
        SummaryFunction::Count => records.len() as f64,
        SummaryFunction::Sum => calculate_numeric_sum(&source_col, records),
        SummaryFunction::Average => {
            if records.is_empty() {
                0.0
            } else {
                calculate_numeric_sum(&source_col, records) / (records.len() as f64)
            }
        }
        SummaryFunction::Min => {
            records
                .iter()
                .filter_map(|r| r.values.get(&source_col).and_then(|v| v.trim().parse::<f64>().ok()))
                .fold(f64::INFINITY, f64::min)
        }
        SummaryFunction::Max => {
            records
                .iter()
                .filter_map(|r| r.values.get(&source_col).and_then(|v| v.trim().parse::<f64>().ok()))
                .fold(f64::NEG_INFINITY, f64::max)
        }
        SummaryFunction::First => {
            records
                .first()
                .and_then(|r| r.values.get(&source_col).and_then(|v| v.trim().parse::<f64>().ok()))
                .unwrap_or(0.0)
        }
        SummaryFunction::Last => {
            records
                .last()
                .and_then(|r| r.values.get(&source_col).and_then(|v| v.trim().parse::<f64>().ok()))
                .unwrap_or(0.0)
        }
        SummaryFunction::Median => {
            let mut nums: Vec<f64> = records
                .iter()
                .filter_map(|r| r.values.get(&source_col).and_then(|v| v.trim().parse::<f64>().ok()))
                .collect();
            if nums.is_empty() {
                0.0
            } else {
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap());
                nums[nums.len() / 2]
            }
        }
        SummaryFunction::PercentOfTotal => 0.0,
    }
}

fn calculate_numeric_sum(col: &str, records: &[ReportRecord]) -> f64 {
    let col_upper = col.to_ascii_uppercase();
    records
        .iter()
        .filter_map(|r| {
            r.values
                .get(&col_upper)
                .or_else(|| r.values.get(col))
                .and_then(|v| v.trim().parse::<f64>().ok())
        })
        .sum()
}
