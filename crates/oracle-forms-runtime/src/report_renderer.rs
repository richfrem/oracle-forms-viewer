use oracle_forms_core::report::*;
use crate::report_evaluator::{ReportExecutionResult, ReportRecord};

pub struct ReportRenderer;

impl ReportRenderer {
    pub fn render_html(report: &ReportDefinition, result: &ReportExecutionResult) -> String {
        let mut html = String::new();

        html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
        html.push_str("  <meta charset=\"UTF-8\">\n");
        html.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
        html.push_str(&format!("  <title>{}</title>\n", escape_html(&result.title)));
        html.push_str("  <style>\n");
        html.push_str(DEFAULT_REPORT_CSS);
        html.push_str("  </style>\n</head>\n<body>\n");

        // Action Toolbar
        html.push_str("  <div class=\"report-toolbar no-print\">\n");
        html.push_str("    <div class=\"toolbar-info\">\n");
        html.push_str(&format!("      <span class=\"report-badge\">Oracle Report</span> <strong>{}</strong>\n", escape_html(&result.report_name)));
        html.push_str("    </div>\n");
        html.push_str("    <div class=\"toolbar-actions\">\n");
        html.push_str("      <button onclick=\"window.print()\" class=\"btn-print\">🖨️ Print / Save as PDF</button>\n");
        html.push_str("    </div>\n");
        html.push_str("  </div>\n\n");

        // Report Sheet
        html.push_str("  <div class=\"report-sheet\">\n");

        for section in &report.layout.sections {
            for node in &section.body {
                render_node(&mut html, node, result, None);
            }
        }

        html.push_str("  </div>\n</body>\n</html>\n");
        html
    }
}

fn render_node(
    html: &mut String,
    node: &ReportLayoutNode,
    result: &ReportExecutionResult,
    current_rec: Option<&ReportRecord>,
) {
    match node {
        ReportLayoutNode::Text(text) => {
            html.push_str(&format!(
                "    <div class=\"report-header-title\">{}</div>\n",
                escape_html(&text.text)
            ));
        }
        ReportLayoutNode::Frame(frame) => {
            let is_summary_frame = frame.name.contains("SUMMARY") || frame.name.contains("TOTAL");
            let frame_class = if is_summary_frame {
                "report-frame summary-frame"
            } else {
                "report-frame"
            };

            html.push_str(&format!("    <div class=\"{}\">\n", frame_class));
            for child in &frame.children {
                render_node(html, child, result, current_rec);
            }
            html.push_str("    </div>\n");
        }
        ReportLayoutNode::RepeatingFrame(rf) => {
            render_repeating_frame(html, rf, result, current_rec);
        }
        ReportLayoutNode::Field(field) => {
            render_field(html, field, result, current_rec);
        }
    }
}

fn render_repeating_frame(
    html: &mut String,
    rf: &ReportRepeatingFrame,
    result: &ReportExecutionResult,
    parent_rec: Option<&ReportRecord>,
) {
    let grp_name = &rf.source;

    // If inside a parent record, get child records for this group, else get top-level group records
    let records_opt = if let Some(parent) = parent_rec {
        parent.child_records.get(grp_name)
    } else {
        result.group_records.get(grp_name)
    };

    let records = match records_opt {
        Some(r) => r.as_slice(),
        None => &[],
    };

    // Check if repeating frame contains nested repeating frames (Master-Detail pattern)
    let has_nested_repeating = rf
        .children
        .iter()
        .any(|c| matches!(c, ReportLayoutNode::RepeatingFrame(_)));

    if has_nested_repeating {
        // Master card layout
        html.push_str(&format!("    <div class=\"master-group-container\" data-group=\"{}\">\n", escape_html(grp_name)));

        for rec in records {
            html.push_str("      <div class=\"master-record-card\">\n");

            // Render master row fields
            html.push_str("        <div class=\"master-header-row\">\n");
            for child in &rf.children {
                if let ReportLayoutNode::Field(f) = child {
                    render_field(html, f, result, Some(rec));
                }
            }
            html.push_str("        </div>\n");

            // Render nested detail repeating frames
            for child in &rf.children {
                if let ReportLayoutNode::RepeatingFrame(nested_rf) = child {
                    render_repeating_frame(html, nested_rf, result, Some(rec));
                }
            }

            // Render group summary footer if present
            html.push_str("        <div class=\"group-summary-row\">\n");
            for child in &rf.children {
                if let ReportLayoutNode::Field(f) = child {
                    if f.source.starts_with("CS_") {
                        let formatted = if let Some(val) = rec.group_summaries.get(&f.source) {
                            format_numeric_value(*val, f.format_mask.as_deref())
                        } else {
                            "—".to_string()
                        };
                        let label = f.source.trim_start_matches("CS_").replace('_', " ");
                        html.push_str(&format!(
                            "          <div class=\"summary-item\"><span class=\"sum-label\">{}:</span> <span class=\"sum-val\">{}</span></div>\n",
                            escape_html(&label),
                            escape_html(&formatted)
                        ));
                    }
                }
            }
            html.push_str("        </div>\n");

            html.push_str("      </div>\n");
        }

        html.push_str("    </div>\n");
    } else {
        // Tabular repeating frame
        html.push_str(&format!("    <table class=\"report-table\" data-group=\"{}\">\n", escape_html(grp_name)));

        // Extract fields in this repeating frame
        let fields: Vec<&ReportField> = rf
            .children
            .iter()
            .filter_map(|c| match c {
                ReportLayoutNode::Field(f) => Some(f),
                _ => None,
            })
            .collect();

        // Table Header
        html.push_str("      <thead>\n        <tr>\n");
        for f in &fields {
            let col_title = f.source.replace('_', " ");
            let align_class = if is_numeric_field(f) { "text-right" } else { "text-left" };
            html.push_str(&format!("          <th class=\"{}\">{}</th>\n", align_class, escape_html(&col_title)));
        }
        html.push_str("        </tr>\n      </thead>\n");

        // Table Body
        html.push_str("      <tbody>\n");
        for rec in records {
            html.push_str("        <tr>\n");
            for f in &fields {
                let raw_val = rec.values.get(&f.source).or_else(|| rec.values.get(&f.source.to_ascii_uppercase()));
                let formatted = match raw_val {
                    Some(s) => format_field_string(s, f.format_mask.as_deref()),
                    None => "—".to_string(),
                };
                let align_class = if is_numeric_field(f) { "text-right" } else { "text-left" };
                html.push_str(&format!("          <td class=\"{}\">{}</td>\n", align_class, escape_html(&formatted)));
            }
            html.push_str("        </tr>\n");
        }
        html.push_str("      </tbody>\n");
        html.push_str("    </table>\n");
    }
}

fn render_field(
    html: &mut String,
    field: &ReportField,
    result: &ReportExecutionResult,
    current_rec: Option<&ReportRecord>,
) {
    let source = &field.source;
    let label = source.trim_start_matches("CS_").trim_start_matches("CF_").replace('_', " ");

    let formatted_val = if source.starts_with("CS_") {
        if let Some(rec) = current_rec {
            if let Some(val) = rec.group_summaries.get(source) {
                format_numeric_value(*val, field.format_mask.as_deref())
            } else if let Some(val) = result.report_summaries.get(source) {
                format_numeric_value(*val, field.format_mask.as_deref())
            } else {
                "—".to_string()
            }
        } else if let Some(val) = result.report_summaries.get(source) {
            format_numeric_value(*val, field.format_mask.as_deref())
        } else {
            "—".to_string()
        }
    } else if let Some(rec) = current_rec {
        rec.values
            .get(source)
            .or_else(|| rec.values.get(&source.to_ascii_uppercase()))
            .map(|s| format_field_string(s, field.format_mask.as_deref()))
            .unwrap_or_else(|| "—".to_string())
    } else {
        "—".to_string()
    };

    html.push_str(&format!(
        "      <div class=\"field-container\"><span class=\"field-label\">{}:</span> <span class=\"field-value\">{}</span></div>\n",
        escape_html(&label),
        escape_html(&formatted_val)
    ));
}

fn is_numeric_field(f: &ReportField) -> bool {
    let s = f.source.to_ascii_uppercase();
    s.contains("TOTAL")
        || s.contains("AMOUNT")
        || s.contains("SALARY")
        || s.contains("COUNT")
        || s.contains("PRICE")
        || s.contains("AVG")
        || s.contains("BONUS")
        || s.starts_with("CS_")
}

fn format_numeric_value(val: f64, mask: Option<&str>) -> String {
    let formatted = format_grouped_number(val);
    if let Some(m) = mask {
        if m.contains('$') {
            return format!("${}", formatted);
        }
    }
    formatted
}

fn format_field_string(s: &str, mask: Option<&str>) -> String {
    let clean = s.trim();
    if let Ok(num) = clean.parse::<f64>() {
        if let Some(m) = mask {
            let formatted = format_grouped_number(num);
            if m.contains('$') {
                return format!("${}", formatted);
            }
            return formatted;
        }
    }
    clean.to_string()
}

fn format_grouped_number(val: f64) -> String {
    let int_part = val.trunc().abs() as i64;
    let fract_part = (val.fract().abs() * 100.0).round() as i64;
    let s = int_part.to_string();
    let mut with_commas = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            with_commas.push(',');
        }
        with_commas.push(*c);
    }
    let sign = if val < 0.0 { "-" } else { "" };
    format!("{}{}.{:02}", sign, with_commas, fract_part)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const DEFAULT_REPORT_CSS: &str = r#"
@page {
  size: letter portrait;
  margin: 0.5in;
  @bottom-right {
    content: "Page " counter(page);
    font-size: 9pt;
    color: #6b7280;
  }
}

* {
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  color: #111827;
  background-color: #f3f4f6;
  margin: 0;
  padding: 24px;
}

.report-toolbar {
  max-width: 900px;
  margin: 0 auto 16px auto;
  display: flex;
  justify-content: space-between;
  align-items: center;
  background: #ffffff;
  padding: 12px 20px;
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0,0,0,0.1);
}

.report-badge {
  background: #2563eb;
  color: white;
  padding: 3px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  margin-right: 8px;
}

.btn-print {
  background: #1e40af;
  color: white;
  border: none;
  padding: 8px 16px;
  border-radius: 6px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s ease;
}

.btn-print:hover {
  background: #1d4ed8;
}

.report-sheet {
  max-width: 900px;
  margin: 0 auto;
  background: #ffffff;
  padding: 40px;
  border-radius: 4px;
  box-shadow: 0 4px 6px -1px rgba(0,0,0,0.1), 0 2px 4px -1px rgba(0,0,0,0.06);
}

.report-header-title {
  font-size: 24px;
  font-weight: 700;
  color: #1e3a8a;
  border-bottom: 2px solid #3b82f6;
  padding-bottom: 8px;
  margin-bottom: 24px;
}

.report-frame {
  margin-bottom: 24px;
}

.summary-frame {
  background: #f8fafc;
  border: 1px solid #e2e8f0;
  border-radius: 6px;
  padding: 16px 20px;
  display: flex;
  flex-wrap: wrap;
  gap: 24px;
  justify-content: flex-end;
}

.report-table {
  width: 100%;
  border-collapse: collapse;
  margin-bottom: 24px;
  font-size: 13px;
}

.report-table th {
  background: #f1f5f9;
  color: #334155;
  font-weight: 600;
  padding: 10px 12px;
  border-bottom: 2px solid #cbd5e1;
}

.report-table td {
  padding: 8px 12px;
  border-bottom: 1px solid #e2e8f0;
  color: #1e293b;
}

.report-table tr:nth-child(even) td {
  background: #fafafa;
}

.text-left { text-align: left; }
.text-right { text-align: right; }

.master-group-container {
  display: flex;
  flex-direction: column;
  gap: 20px;
  margin-bottom: 24px;
}

.master-record-card {
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  background: #ffffff;
  overflow: hidden;
}

.master-header-row {
  background: #f1f5f9;
  padding: 12px 16px;
  font-weight: 600;
  display: flex;
  gap: 24px;
  border-bottom: 1px solid #cbd5e1;
}

.group-summary-row {
  background: #f8fafc;
  padding: 10px 16px;
  display: flex;
  justify-content: flex-end;
  gap: 20px;
  border-top: 1px solid #e2e8f0;
  font-weight: 600;
}

.field-container {
  display: inline-flex;
  gap: 6px;
}

.field-label, .sum-label {
  color: #64748b;
  font-weight: 500;
}

.field-value, .sum-val {
  color: #0f172a;
}

@media print {
  body {
    background: transparent;
    padding: 0;
  }
  .report-sheet {
    box-shadow: none;
    padding: 0;
    max-width: 100%;
  }
  .no-print {
    display: none !important;
  }
}
"#;
