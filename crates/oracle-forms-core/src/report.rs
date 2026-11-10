use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportDefinition {
    pub name: String,
    pub title: Option<String>,
    pub dtd_version: Option<String>,
    pub unit_of_measurement: Option<String>,
    pub data: ReportDataModel,
    pub layout: ReportLayout,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportDataModel {
    pub data_sources: Vec<ReportDataSource>,
    pub links: Vec<ReportLink>,
    pub parameters: Vec<ReportParameter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportDataSource {
    pub name: String,
    pub select_sql: String,
    pub groups: Vec<ReportGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportGroup {
    pub name: String,
    pub data_items: Vec<ReportDataItem>,
    pub summaries: Vec<ReportSummary>,
    pub formulas: Vec<ReportFormula>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportDataItem {
    pub name: String,
    pub datatype: String,
    pub width: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryFunction {
    Sum,
    Average,
    Count,
    Min,
    Max,
    Median,
    PercentOfTotal,
    First,
    Last,
}

impl SummaryFunction {
    pub fn from_str_name(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "sum" => SummaryFunction::Sum,
            "average" | "avg" => SummaryFunction::Average,
            "count" => SummaryFunction::Count,
            "minimum" | "min" => SummaryFunction::Min,
            "maximum" | "max" => SummaryFunction::Max,
            "median" => SummaryFunction::Median,
            "percentoftotal" | "percent_of_total" => SummaryFunction::PercentOfTotal,
            "first" => SummaryFunction::First,
            "last" => SummaryFunction::Last,
            _ => SummaryFunction::Sum,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportSummary {
    pub name: String,
    pub source: String,
    pub function: SummaryFunction,
    pub reset: Option<String>,
    pub compute: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportFormula {
    pub name: String,
    pub source: Option<String>,
    pub datatype: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportLink {
    pub name: Option<String>,
    pub parent_group: String,
    pub parent_column: String,
    pub child_query: String,
    pub child_column: String,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportParameter {
    pub name: String,
    pub datatype: Option<String>,
    pub initial_value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportLayout {
    pub sections: Vec<ReportSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportSection {
    pub name: Option<String>,
    pub body: Vec<ReportLayoutNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReportLayoutNode {
    Frame(ReportFrame),
    RepeatingFrame(ReportRepeatingFrame),
    Field(ReportField),
    Text(ReportText),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportFrame {
    pub name: String,
    pub geometry: Option<ReportGeometry>,
    pub children: Vec<ReportLayoutNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportRepeatingFrame {
    pub name: String,
    pub source: String,
    pub print_direction: String,
    pub geometry: Option<ReportGeometry>,
    pub children: Vec<ReportLayoutNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportField {
    pub name: String,
    pub source: String,
    pub format_mask: Option<String>,
    pub geometry: Option<ReportGeometry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportText {
    pub name: String,
    pub text: String,
    pub geometry: Option<ReportGeometry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReportGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
