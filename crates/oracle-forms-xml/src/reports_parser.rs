use std::fs;
use std::path::Path;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use oracle_forms_core::error::{FormsError, Result};
use oracle_forms_core::report::*;

pub struct ReportsXmlParser;

enum LayoutContainer {
    Section {
        name: Option<String>,
        body: Vec<ReportLayoutNode>,
    },
    Frame(ReportFrame),
    RepeatingFrame(ReportRepeatingFrame),
}

impl ReportsXmlParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<ReportDefinition> {
        let content = fs::read_to_string(path)?;
        Self::parse_str(&content)
    }

    pub fn parse_str(xml_str: &str) -> Result<ReportDefinition> {
        let mut reader = Reader::from_str(xml_str);
        reader.config_mut().trim_text(true);

        let mut report_name = String::from("UNNAMED_REPORT");
        let mut report_title = None;
        let mut dtd_version = None;
        let mut unit_of_measurement = None;

        let mut data_model = ReportDataModel::default();
        let mut layout = ReportLayout::default();

        // Data model parsing state
        let mut current_data_source: Option<ReportDataSource> = None;
        let mut current_group: Option<ReportGroup> = None;
        let mut in_select = false;
        let mut select_sql_buffer = String::new();

        let mut container_stack: Vec<LayoutContainer> = Vec::new();
        let mut current_field: Option<ReportField> = None;
        let mut current_text: Option<ReportText> = None;
        let mut in_string = false;
        let mut text_buffer = String::new();

        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let tag_lower = tag_name.to_ascii_lowercase();

                    match tag_lower.as_str() {
                        "report" => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => report_name = val,
                                    "title" => report_title = Some(val),
                                    "dtdversion" => dtd_version = Some(val),
                                    "unitofmeasurement" => unit_of_measurement = Some(val),
                                    _ => {}
                                }
                            }
                        }
                        "datasource" => {
                            let mut ds_name = String::new();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "name" {
                                    ds_name = val;
                                }
                            }
                            current_data_source = Some(ReportDataSource {
                                name: ds_name,
                                select_sql: String::new(),
                                groups: Vec::new(),
                            });
                        }
                        "select" => {
                            in_select = true;
                            select_sql_buffer.clear();
                        }
                        "group" => {
                            let mut grp_name = String::new();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "name" {
                                    grp_name = val;
                                }
                            }
                            current_group = Some(ReportGroup {
                                name: grp_name,
                                data_items: Vec::new(),
                                summaries: Vec::new(),
                                formulas: Vec::new(),
                            });
                        }
                        "section" => {
                            let mut sec_name = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "name" {
                                    sec_name = Some(val);
                                }
                            }
                            container_stack.push(LayoutContainer::Section {
                                name: sec_name,
                                body: Vec::new(),
                            });
                        }
                        "frame" => {
                            let mut frame_name = String::new();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "name" {
                                    frame_name = val;
                                }
                            }
                            container_stack.push(LayoutContainer::Frame(ReportFrame {
                                name: frame_name,
                                geometry: None,
                                children: Vec::new(),
                            }));
                        }
                        "repeatingframe" => {
                            let mut r_name = String::new();
                            let mut source = String::new();
                            let mut print_dir = String::from("down");
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => r_name = val,
                                    "source" => source = val,
                                    "printdirection" => print_dir = val,
                                    _ => {}
                                }
                            }
                            container_stack.push(LayoutContainer::RepeatingFrame(ReportRepeatingFrame {
                                name: r_name,
                                source,
                                print_direction: print_dir,
                                geometry: None,
                                children: Vec::new(),
                            }));
                        }
                        "field" => {
                            let mut f_name = String::new();
                            let mut source = String::new();
                            let mut format_mask = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => f_name = val,
                                    "source" => source = val,
                                    "formatmask" => format_mask = Some(val),
                                    _ => {}
                                }
                            }
                            current_field = Some(ReportField {
                                name: f_name,
                                source,
                                format_mask,
                                geometry: None,
                            });
                        }
                        "text" => {
                            let mut t_name = String::new();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "name" {
                                    t_name = val;
                                }
                            }
                            current_text = Some(ReportText {
                                name: t_name,
                                text: String::new(),
                                geometry: None,
                            });
                            text_buffer.clear();
                        }
                        "string" => {
                            in_string = true;
                            text_buffer.clear();
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let tag_lower = tag_name.to_ascii_lowercase();

                    match tag_lower.as_str() {
                        "dataitem" => {
                            let mut name = String::new();
                            let mut datatype = String::from("character");
                            let mut width = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => name = val,
                                    "datatype" => datatype = val,
                                    "width" => width = val.parse::<u32>().ok(),
                                    _ => {}
                                }
                            }
                            if let Some(ref mut grp) = current_group {
                                grp.data_items.push(ReportDataItem { name, datatype, width });
                            }
                        }
                        "summary" => {
                            let mut name = String::new();
                            let mut source = String::new();
                            let mut function_str = String::from("sum");
                            let mut reset = None;
                            let mut compute = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => name = val,
                                    "source" => source = val,
                                    "function" => function_str = val,
                                    "reset" => reset = Some(val),
                                    "compute" => compute = Some(val),
                                    _ => {}
                                }
                            }
                            let summary = ReportSummary {
                                name,
                                source,
                                function: SummaryFunction::from_str_name(&function_str),
                                reset,
                                compute,
                            };
                            if let Some(ref mut grp) = current_group {
                                grp.summaries.push(summary);
                            }
                        }
                        "formula" => {
                            let mut name = String::new();
                            let mut source = None;
                            let mut datatype = String::from("number");
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => name = val,
                                    "source" => source = Some(val),
                                    "datatype" => datatype = val,
                                    _ => {}
                                }
                            }
                            let formula = ReportFormula { name, source, datatype };
                            if let Some(ref mut grp) = current_group {
                                grp.formulas.push(formula);
                            }
                        }
                        "link" => {
                            let mut name = None;
                            let mut parent_group = String::new();
                            let mut parent_column = String::new();
                            let mut child_query = String::new();
                            let mut child_column = String::new();
                            let mut condition = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => name = Some(val),
                                    "parentgroup" => parent_group = val,
                                    "parentcolumn" => parent_column = val,
                                    "childquery" => child_query = val,
                                    "childcolumn" => child_column = val,
                                    "condition" => condition = Some(val),
                                    _ => {}
                                }
                            }
                            data_model.links.push(ReportLink {
                                name,
                                parent_group,
                                parent_column,
                                child_query,
                                child_column,
                                condition,
                            });
                        }
                        "userparameter" | "systemparameter" => {
                            let mut name = String::new();
                            let mut datatype = None;
                            let mut initial_value = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => name = val,
                                    "datatype" => datatype = Some(val),
                                    "initialvalue" => initial_value = Some(val),
                                    _ => {}
                                }
                            }
                            data_model.parameters.push(ReportParameter {
                                name,
                                datatype,
                                initial_value,
                            });
                        }
                        "geometryinfo" => {
                            let mut x = 0.0f32;
                            let mut y = 0.0f32;
                            let mut width = 0.0f32;
                            let mut height = 0.0f32;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value);
                                if let Ok(n) = val.parse::<f32>() {
                                    match key.as_str() {
                                        "x" => x = n,
                                        "y" => y = n,
                                        "width" => width = n,
                                        "height" => height = n,
                                        _ => {}
                                    }
                                }
                            }
                            let geom = ReportGeometry { x, y, width, height };

                            if let Some(ref mut f) = current_field {
                                f.geometry = Some(geom);
                            } else if let Some(ref mut t) = current_text {
                                t.geometry = Some(geom);
                            } else if let Some(last_container) = container_stack.last_mut() {
                                match last_container {
                                    LayoutContainer::Frame(ref mut frame) => frame.geometry = Some(geom),
                                    LayoutContainer::RepeatingFrame(ref mut r_frame) => r_frame.geometry = Some(geom),
                                    LayoutContainer::Section { .. } => {}
                                }
                            }
                        }
                        "field" => {
                            let mut f_name = String::new();
                            let mut source = String::new();
                            let mut format_mask = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "name" => f_name = val,
                                    "source" => source = val,
                                    "formatmask" => format_mask = Some(val),
                                    _ => {}
                                }
                            }
                            let field_node = ReportLayoutNode::Field(ReportField {
                                name: f_name,
                                source,
                                format_mask,
                                geometry: None,
                            });
                            append_to_top_container(&mut container_stack, field_node);
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    let text = e.unescape().map(|c| c.to_string()).unwrap_or_default();
                    if in_select {
                        select_sql_buffer.push_str(&text);
                    } else if in_string {
                        text_buffer.push_str(&text);
                    }
                }
                Ok(Event::End(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let tag_lower = tag_name.to_ascii_lowercase();

                    match tag_lower.as_str() {
                        "select" => {
                            in_select = false;
                            if let Some(ref mut ds) = current_data_source {
                                ds.select_sql = select_sql_buffer.trim().to_string();
                            }
                        }
                        "group" => {
                            if let Some(grp) = current_group.take() {
                                if let Some(ref mut ds) = current_data_source {
                                    ds.groups.push(grp);
                                }
                            }
                        }
                        "datasource" => {
                            if let Some(ds) = current_data_source.take() {
                                data_model.data_sources.push(ds);
                            }
                        }
                        "field" => {
                            if let Some(field) = current_field.take() {
                                append_to_top_container(&mut container_stack, ReportLayoutNode::Field(field));
                            }
                        }
                        "string" => {
                            in_string = false;
                            if let Some(ref mut t) = current_text {
                                t.text = text_buffer.trim().to_string();
                            }
                        }
                        "text" => {
                            if let Some(mut text_obj) = current_text.take() {
                                if text_obj.text.is_empty() && !text_buffer.trim().is_empty() {
                                    text_obj.text = text_buffer.trim().to_string();
                                }
                                append_to_top_container(&mut container_stack, ReportLayoutNode::Text(text_obj));
                            }
                        }
                        "frame" => {
                            if let Some(LayoutContainer::Frame(frame)) = container_stack.pop() {
                                append_to_top_container(&mut container_stack, ReportLayoutNode::Frame(frame));
                            }
                        }
                        "repeatingframe" => {
                            if let Some(LayoutContainer::RepeatingFrame(r_frame)) = container_stack.pop() {
                                append_to_top_container(&mut container_stack, ReportLayoutNode::RepeatingFrame(r_frame));
                            }
                        }
                        "section" => {
                            if let Some(LayoutContainer::Section { name, body }) = container_stack.pop() {
                                layout.sections.push(ReportSection { name, body });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(err) => return Err(FormsError::Xml(format!("Error parsing reports XML: {}", err))),
                _ => {}
            }
            buf.clear();
        }

        Ok(ReportDefinition {
            name: report_name,
            title: report_title,
            dtd_version,
            unit_of_measurement,
            data: data_model,
            layout,
        })
    }
}

fn append_to_top_container(
    stack: &mut [LayoutContainer],
    node: ReportLayoutNode,
) {
    if let Some(top) = stack.last_mut() {
        match top {
            LayoutContainer::Section { body, .. } => body.push(node),
            LayoutContainer::Frame(f) => f.children.push(node),
            LayoutContainer::RepeatingFrame(rf) => rf.children.push(node),
        }
    }
}
