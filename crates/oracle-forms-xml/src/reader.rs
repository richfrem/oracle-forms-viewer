use std::collections::HashSet;
use std::fs;
use std::path::Path;
use quick_xml::events::BytesStart;
use quick_xml::reader::Reader;
use oracle_forms_core::error::{FormsError, Result};
use oracle_forms_core::model::*;
use oracle_forms_core::sidecars::{
    cache_relative, decode_double_escaped, first_code_line, line_count_of, PlsqlSidecars,
};
use crate::pld::chrono_iso_now;

struct Frame {
    element: String,
    name: Option<String>,
    start_line: usize,
    owner_path: Option<String>,
    inherited: Option<InheritanceRef>,
}

struct BlockBuilder {
    name: String,
    query_data_source_name: Option<String>,
    property_class: Option<String>,
    inherited: Option<InheritanceRef>,
    dml: Option<BlockDml>,
    records_displayed: i32,
    distance_between_records: i32,
    record_orientation: String,
    items: Vec<ItemInfo>,
    trigger_names: Vec<String>,
    relations: Vec<RelationInfo>,
    data_source_column_count: i32,
}

struct ItemBuilder {
    name: String,
    item_type: Option<String>,
    data_type: Option<String>,
    column_name: Option<String>,
    canvas_name: Option<String>,
    prompt: Option<String>,
    property_class: Option<String>,
    visible: Option<bool>,
    required: Option<bool>,
    lov_name: Option<String>,
    inherited: Option<InheritanceRef>,
    dml: Option<ItemDml>,
    width: Option<i32>,
    height: Option<i32>,
    x_position: i32,
    y_position: i32,
    tab_page_name: Option<String>,
    label: Option<String>,
    prompt_attachment_edge: String,
    prompt_attachment_offset: i32,
    prompt_align: String,
    items_display: i32,
    distance_between_records: i32,
    back_color: Option<String>,
    fore_color: Option<String>,
    enabled: bool,
    canvas_visible: bool,
    trigger_names: Vec<String>,
}

struct CanvasBuilder {
    name: String,
    canvas_type: Option<String>,
    window_name: Option<String>,
    property_class: Option<String>,
    raise_on_enter: Option<bool>,
    width: i32,
    height: i32,
    viewport_width: Option<i32>,
    viewport_height: Option<i32>,
    viewport_x_position: i32,
    viewport_y_position: i32,
    back_color: Option<String>,
    tab_pages: Vec<TabPageInfo>,
}

pub struct FormsXmlParser;

impl FormsXmlParser {
    pub fn parse(key: &ModuleKey, converted_file: &Path, module_cache_dir: &Path) -> Result<ModuleIndex> {
        let content_bytes = fs::read(converted_file)?;
        let xml_path = cache_relative(converted_file, module_cache_dir);
        let mut sidecars = PlsqlSidecars::new(module_cache_dir);

        // Precompute line start byte offsets
        let mut line_starts = vec![0usize];
        for (idx, &b) in content_bytes.iter().enumerate() {
            if b == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        let byte_to_line = |pos: usize| -> usize {
            match line_starts.binary_search(&pos) {
                Ok(idx) => idx + 1,
                Err(idx) => idx,
            }
        };

        let mut forms_version: Option<String> = None;
        let mut module_name: String = key.name.clone();
        let mut coordinate_info = CoordinateInfo::default();
        let mut blocks: Vec<BlockInfo> = Vec::new();
        let mut triggers: Vec<TriggerInfo> = Vec::new();
        let mut program_units: Vec<ProgramUnitInfo> = Vec::new();
        let mut attached_libraries: Vec<AttachedLibraryInfo> = Vec::new();
        let mut lovs: Vec<LovInfo> = Vec::new();
        let mut record_groups: Vec<RecordGroupInfo> = Vec::new();
        let mut windows: Vec<WindowInfo> = Vec::new();
        let mut canvases: Vec<CanvasInfo> = Vec::new();
        let mut graphics: Vec<GraphicInfo> = Vec::new();
        let mut alerts: Vec<AlertInfo> = Vec::new();
        let mut parameters: Vec<ParameterInfo> = Vec::new();
        let mut visual_attributes: Vec<String> = Vec::new();
        let mut property_classes: Vec<String> = Vec::new();
        let mut property_class_details: Vec<PropertyClassInfo> = Vec::new();
        let mut editors: Vec<String> = Vec::new();
        let mut menu_module: Option<String> = None;
        let mut menus: Vec<MenuInfo> = Vec::new();
        let mut object_library_tabs: Vec<ObjectLibraryTabInfo> = Vec::new();
        let mut object_refs: Vec<ObjectRef> = Vec::new();

        let mut current_block: Option<BlockBuilder> = None;
        let mut current_item: Option<ItemBuilder> = None;
        let mut current_canvas: Option<CanvasBuilder> = None;
        let mut current_tab_page_name: Option<String> = None;
        let mut current_lov: Option<LovInfo> = None;
        let mut current_record_group: Option<RecordGroupInfo> = None;
        let mut current_menu: Option<MenuInfo> = None;
        let mut current_tab: Option<ObjectLibraryTabInfo> = None;

        let mut stack: Vec<Frame> = Vec::new();

        let mut reader = Reader::from_reader(content_bytes.as_slice());
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();

        loop {
            let start_pos = reader.buffer_position() as usize;
            let event = match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(e) => e,
                Err(e) => return Err(FormsError::Xml(format!("XML parse error at byte {}: {:?}", start_pos, e))),
            };
            let end_pos = reader.buffer_position() as usize;
            let start_line = byte_to_line(start_pos);
            let end_line = byte_to_line(end_pos).max(start_line);

            match event {
                quick_xml::events::Event::Start(ref e) => {
                    Self::handle_start_element(
                        e,
                        start_line,
                        &mut stack,
                        &mut forms_version,
                        &mut module_name,
                        &mut coordinate_info,
                        &mut current_block,
                        &mut current_item,
                        &mut current_canvas,
                        &mut current_tab_page_name,
                        &mut graphics,
                        &mut triggers,
                        &mut program_units,
                        &mut attached_libraries,
                        &mut current_lov,
                        &mut current_record_group,
                        &mut windows,
                        &mut alerts,
                        &mut parameters,
                        &mut visual_attributes,
                        &mut property_classes,
                        &mut property_class_details,
                        &mut editors,
                        &mut menu_module,
                        &mut current_menu,
                        &mut current_tab,
                        &mut sidecars,
                    )?;
                }
                quick_xml::events::Event::Empty(ref e) => {
                    Self::handle_start_element(
                        e,
                        start_line,
                        &mut stack,
                        &mut forms_version,
                        &mut module_name,
                        &mut coordinate_info,
                        &mut current_block,
                        &mut current_item,
                        &mut current_canvas,
                        &mut current_tab_page_name,
                        &mut graphics,
                        &mut triggers,
                        &mut program_units,
                        &mut attached_libraries,
                        &mut current_lov,
                        &mut current_record_group,
                        &mut windows,
                        &mut alerts,
                        &mut parameters,
                        &mut visual_attributes,
                        &mut property_classes,
                        &mut property_class_details,
                        &mut editors,
                        &mut menu_module,
                        &mut current_menu,
                        &mut current_tab,
                        &mut sidecars,
                    )?;
                    Self::handle_end_element(
                        end_line,
                        &xml_path,
                        &mut stack,
                        &mut object_refs,
                        &mut triggers,
                        &mut program_units,
                        &mut current_block,
                        &mut current_item,
                        &mut blocks,
                        &mut current_canvas,
                        &mut canvases,
                        &mut current_tab_page_name,
                        &mut current_lov,
                        &mut lovs,
                        &mut current_record_group,
                        &mut record_groups,
                        &mut current_menu,
                        &mut menus,
                        &mut current_tab,
                        &mut object_library_tabs,
                    );
                }
                quick_xml::events::Event::End(_) => {
                    Self::handle_end_element(
                        end_line,
                        &xml_path,
                        &mut stack,
                        &mut object_refs,
                        &mut triggers,
                        &mut program_units,
                        &mut current_block,
                        &mut current_item,
                        &mut blocks,
                        &mut current_canvas,
                        &mut canvases,
                        &mut current_tab_page_name,
                        &mut current_lov,
                        &mut lovs,
                        &mut current_record_group,
                        &mut record_groups,
                        &mut current_menu,
                        &mut menus,
                        &mut current_tab,
                        &mut object_library_tabs,
                    );
                }
                _ => {}
            }
            buf.clear();
        }

        // Post-process property classes: only retain propertyClass if declared
        let declared: HashSet<String> = property_classes.iter().map(|s| s.to_ascii_uppercase()).collect();
        let classed_blocks = blocks.into_iter().map(|mut b| {
            b.property_class = b.property_class.filter(|s| declared.contains(&s.to_ascii_uppercase()));
            b.items = b.items.into_iter().map(|mut it| {
                it.property_class = it.property_class.filter(|s| declared.contains(&s.to_ascii_uppercase()));
                it
            }).collect();
            b
        }).collect();

        let classed_canvases = canvases.into_iter().map(|mut c| {
            c.property_class = c.property_class.filter(|s| declared.contains(&s.to_ascii_uppercase()));
            c
        }).collect();

        let classed_details = property_class_details.into_iter().map(|mut d| {
            d.property_class = d.property_class.filter(|s| declared.contains(&s.to_ascii_uppercase()));
            d
        }).collect();

        let now = chrono_iso_now();

        Ok(ModuleIndex {
            key: key.clone(),
            forms_version,
            index_version: oracle_forms_core::CURRENT_INDEX_VERSION,
            source_file: converted_file.to_string_lossy().to_string(),
            fingerprint: ModuleFingerprint::default(),
            converted_file: xml_path,
            parsed_at: now,
            coordinate: coordinate_info,
            blocks: classed_blocks,
            triggers,
            program_units,
            attached_libraries,
            lovs,
            record_groups,
            windows,
            canvases: classed_canvases,
            graphics,
            alerts,
            parameters,
            visual_attributes,
            property_classes,
            property_class_details: classed_details,
            editors,
            menu_module,
            menus,
            object_library_tabs,
            object_refs,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_start_element(
        e: &BytesStart,
        start_line: usize,
        stack: &mut Vec<Frame>,
        forms_version: &mut Option<String>,
        module_name: &mut String,
        coordinate_info: &mut CoordinateInfo,
        current_block: &mut Option<BlockBuilder>,
        current_item: &mut Option<ItemBuilder>,
        current_canvas: &mut Option<CanvasBuilder>,
        current_tab_page_name: &mut Option<String>,
        graphics: &mut Vec<GraphicInfo>,
        triggers: &mut Vec<TriggerInfo>,
        program_units: &mut Vec<ProgramUnitInfo>,
        attached_libraries: &mut Vec<AttachedLibraryInfo>,
        current_lov: &mut Option<LovInfo>,
        current_record_group: &mut Option<RecordGroupInfo>,
        windows: &mut Vec<WindowInfo>,
        alerts: &mut Vec<AlertInfo>,
        parameters: &mut Vec<ParameterInfo>,
        visual_attributes: &mut Vec<String>,
        property_classes: &mut Vec<String>,
        property_class_details: &mut Vec<PropertyClassInfo>,
        editors: &mut Vec<String>,
        menu_module: &mut Option<String>,
        current_menu: &mut Option<MenuInfo>,
        current_tab: &mut Option<ObjectLibraryTabInfo>,
        sidecars: &mut PlsqlSidecars,
    ) -> Result<()> {
        let element = String::from_utf8_lossy(e.name().as_ref()).to_string();
        let name = get_attr(e, "Name");
        let parent_element = stack.last().map(|p| p.element.clone());
        let parent_inherited = stack.last().and_then(|p| p.inherited.clone());
        let owner_path = owner_path_of(stack);
        let inherited = inheritance_of(e, name.as_deref(), parent_inherited.as_ref(), module_name);

        stack.push(Frame {
            element: element.clone(),
            name: name.clone(),
            start_line,
            owner_path: owner_path.clone(),
            inherited: inherited.clone(),
        });

        match element.as_str() {
            "Module" => {
                *forms_version = get_attr(e, "version");
            }
            "FormModule" => {
                if let Some(ref n) = name {
                    *module_name = n.clone();
                }
                *menu_module = get_attr(e, "MenuModule");
            }
            "MenuModule" | "ObjectLibrary" => {
                if let Some(ref n) = name {
                    *module_name = n.clone();
                }
            }
            "Coordinate" => {
                *coordinate_info = CoordinateInfo {
                    coordinate_system: get_attr(e, "CoordinateSystem"),
                    real_unit: get_attr(e, "RealUnit").or_else(|| Some("Point".to_string())),
                    character_cell_width: get_int_attr(e, "CharacterCellWidth").unwrap_or(7),
                    character_cell_height: get_int_attr(e, "CharacterCellHeight").unwrap_or(14),
                    default_font_scaling: get_bool_attr(e, "DefaultFontScaling").unwrap_or(true),
                };
            }
            "Block" => {
                if parent_element.as_deref() == Some("FormModule") {
                    let source = get_attr(e, "QueryDataSourceName").map(|s| decode_double_escaped(&s));
                    let records_disp = get_int_attr(e, "RecordsDisplayCount")
                        .or_else(|| get_int_attr(e, "RecordsDisplayed"))
                        .unwrap_or(1);
                    let dist = get_int_attr(e, "DistanceBetweenRecords").unwrap_or(0);
                    let orient = get_attr(e, "RecordOrientation").unwrap_or_else(|| "Vertical".to_string());

                    *current_block = Some(BlockBuilder {
                        name: name.clone().unwrap_or_default(),
                        query_data_source_name: source.as_ref().map(|s| s.text.clone()),
                        property_class: local_parent_name(e, module_name),
                        inherited,
                        dml: block_dml(e, source.as_ref().map(|s| s.encoding == TextEncoding::Recovered).unwrap_or(false)),
                        records_displayed: records_disp,
                        distance_between_records: dist,
                        record_orientation: orient,
                        items: Vec::new(),
                        trigger_names: Vec::new(),
                        relations: Vec::new(),
                        data_source_column_count: 0,
                    });
                }
            }
            "DataSourceColumn" => {
                if parent_element.as_deref() == Some("Block") {
                    if let Some(b) = current_block.as_mut() {
                        b.data_source_column_count += 1;
                    }
                }
            }
            "Relation" => {
                if parent_element.as_deref() == Some("Block") {
                    if let Some(b) = current_block.as_mut() {
                        let join = get_attr(e, "JoinCondition").map(|s| decode_double_escaped(&s));
                        b.relations.push(RelationInfo {
                            name: name.clone().unwrap_or_default(),
                            detail_block: get_attr(e, "DetailBlock"),
                            join_condition: join.as_ref().map(|j| j.text.clone()),
                            join_encoding: join.map(|j| j.encoding).unwrap_or_default(),
                            deferred: get_bool_attr(e, "Deferred"),
                            auto_query: get_bool_attr(e, "AutoQuery"),
                            prevent_masterless_operations: get_bool_attr(e, "PreventMasterlessOperations"),
                            delete_record: get_attr(e, "DeleteRecord"),
                            relation_type: get_attr(e, "RelationType"),
                            inherited,
                        });
                    }
                }
            }
            "Item" => {
                if parent_element.as_deref() == Some("Block") && current_block.is_some() {
                    let vis = get_bool_attr(e, "Visible").unwrap_or(true);
                    let enabled = get_bool_attr(e, "Enabled").unwrap_or(true);
                    let block_records = current_block.as_ref().map(|b| b.records_displayed).unwrap_or(1);
                    let block_dist = current_block.as_ref().map(|b| b.distance_between_records).unwrap_or(0);
                    let items_disp = get_int_attr(e, "ItemsDisplay").unwrap_or(block_records);
                    let dist = get_int_attr(e, "DistanceBetweenRecords").unwrap_or(block_dist);

                    let raw_type = get_attr(e, "ItemType");
                    let parent_name = get_attr(e, "ParentName").unwrap_or_default();
                    let label = get_attr(e, "Label");
                    let prompt = get_attr(e, "Prompt");
                    let name_upper = name.as_deref().unwrap_or("").to_ascii_uppercase();
                    let parent_upper = parent_name.to_ascii_uppercase();

                    let resolved_type = raw_type.or_else(|| {
                        if name_upper.starts_with("BTN")
                            || name_upper.contains("BUTTON")
                            || parent_upper.contains("BUTTON")
                            || (label.is_some() && prompt.is_none())
                        {
                            Some("Push Button".to_string())
                        } else if name_upper.starts_with("CHK") || parent_upper.contains("CHECK") {
                            Some("Check Box".to_string())
                        } else if name_upper.starts_with("RAD") || parent_upper.contains("RADIO") {
                            Some("Radio Button".to_string())
                        } else {
                            None
                        }
                    });

                    let raw_canvas = get_attr(e, "CanvasName").filter(|s| !s.trim().is_empty());

                    *current_item = Some(ItemBuilder {
                        name: name.clone().unwrap_or_default(),
                        item_type: resolved_type,
                        data_type: get_attr(e, "DataType"),
                        column_name: get_attr(e, "ColumnName"),
                        canvas_name: raw_canvas,
                        prompt,
                        property_class: local_parent_name(e, module_name),
                        visible: Some(vis),
                        required: get_bool_attr(e, "Required"),
                        lov_name: get_attr(e, "LOVName").or_else(|| get_attr(e, "LovName")),
                        inherited,
                        dml: item_dml(e),
                        width: get_int_attr(e, "Width"),
                        height: get_int_attr(e, "Height"),
                        x_position: get_int_attr(e, "XPosition").unwrap_or(0),
                        y_position: get_int_attr(e, "YPosition").unwrap_or(0),
                        tab_page_name: get_attr(e, "TabPageName").filter(|s| !s.trim().is_empty()),
                        label,
                        prompt_attachment_edge: get_attr(e, "PromptAttachmentEdge").unwrap_or_else(|| "Start".to_string()),
                        prompt_attachment_offset: get_int_attr(e, "PromptAttachmentOffset").unwrap_or(5),
                        prompt_align: get_attr(e, "PromptAlign").unwrap_or_else(|| "Start".to_string()),
                        items_display: items_disp,
                        distance_between_records: dist,
                        back_color: get_attr(e, "BackColor"),
                        fore_color: get_attr(e, "ForeColor"),
                        enabled,
                        canvas_visible: vis,
                        trigger_names: Vec::new(),
                    });
                }
            }
            "Canvas" => {
                *current_canvas = Some(CanvasBuilder {
                    name: name.clone().unwrap_or_default(),
                    canvas_type: get_attr(e, "CanvasType"),
                    window_name: get_attr(e, "WindowName"),
                    property_class: local_parent_name(e, module_name),
                    raise_on_enter: get_bool_attr(e, "RaiseOnEnter"),
                    width: get_int_attr(e, "Width").unwrap_or(800),
                    height: get_int_attr(e, "Height").unwrap_or(600),
                    viewport_width: get_int_attr(e, "ViewportWidth"),
                    viewport_height: get_int_attr(e, "ViewportHeight"),
                    viewport_x_position: get_int_attr(e, "ViewportXPosition").unwrap_or(0),
                    viewport_y_position: get_int_attr(e, "ViewportYPosition").unwrap_or(0),
                    back_color: get_attr(e, "BackColor"),
                    tab_pages: Vec::new(),
                });
            }
            "TabPage" => {
                let page_name = name.clone().unwrap_or_default();
                let page_label = get_attr(e, "Label");
                *current_tab_page_name = Some(page_name.clone());
                if let Some(c) = current_canvas.as_mut() {
                    c.tab_pages.push(TabPageInfo {
                        name: page_name,
                        label: page_label,
                    });
                }
            }
            "Graphics" => {
                let g_type = get_attr(e, "GraphicsType").unwrap_or_else(|| "Rectangle".to_string());
                let g_text = get_attr(e, "GraphicsText")
                    .map(|s| decode_double_escaped(&s).text)
                    .or_else(|| get_attr(e, "Text"));
                let f_title = get_attr(e, "FrameTitle").or_else(|| get_attr(e, "Title"));

                graphics.push(GraphicInfo {
                    name: name.clone().unwrap_or_else(|| format!("GRAPHIC_{}", graphics.len() + 1)),
                    canvas_name: current_canvas.as_ref().map(|c| c.name.clone()).or_else(|| get_attr(e, "CanvasName")),
                    tab_page_name: current_tab_page_name.clone().or_else(|| get_attr(e, "TabPageName")),
                    graphics_type: g_type,
                    graphics_text: g_text,
                    frame_title: f_title,
                    x_position: get_int_attr(e, "XPosition").unwrap_or(0),
                    y_position: get_int_attr(e, "YPosition").unwrap_or(0),
                    width: get_int_attr(e, "Width").unwrap_or(0),
                    height: get_int_attr(e, "Height").unwrap_or(0),
                    bevel: get_attr(e, "Bevel"),
                    back_color: get_attr(e, "BackColor"),
                });
            }
            "Trigger" => {
                let raw_text = get_attr(e, "TriggerText").unwrap_or_default();
                let body = decode_double_escaped(&raw_text);
                let text = body.text;
                let level = match parent_element.as_deref() {
                    Some("Block") => TriggerLevel::Block,
                    Some("Item") => TriggerLevel::Item,
                    Some("Menu") | Some("MenuItem") => TriggerLevel::Menu,
                    _ => TriggerLevel::Form,
                };
                let trigger_name = name.clone().unwrap_or_default();
                let scope = owner_path.clone().unwrap_or_else(|| "FORM".to_string());
                let text_ref = sidecars.write(
                    PlsqlSidecars::TRIGGERS,
                    &format!("{}.{}", scope, trigger_name),
                    &text,
                )?;

                let block_name = match level {
                    TriggerLevel::Block | TriggerLevel::Item => current_block.as_ref().map(|b| b.name.clone()),
                    _ => None,
                };
                let item_name = match level {
                    TriggerLevel::Item => current_item.as_ref().map(|i| i.name.clone()),
                    _ => None,
                };

                match level {
                    TriggerLevel::Block => {
                        if let Some(b) = current_block.as_mut() {
                            b.trigger_names.push(trigger_name.clone());
                        }
                    }
                    TriggerLevel::Item => {
                        if let Some(i) = current_item.as_mut() {
                            i.trigger_names.push(trigger_name.clone());
                        }
                    }
                    _ => {}
                }

                triggers.push(TriggerInfo {
                    name: trigger_name,
                    level,
                    block_name,
                    item_name,
                    first_line: first_code_line(&text),
                    line_count: line_count_of(&text),
                    inherited,
                    text_encoding: body.encoding,
                    text_ref: Some(text_ref),
                    xml_ref: None,
                    source_text: Some(text.clone()),
                    execution_hierarchy: get_attr(e, "ExecuteHierarchy").or_else(|| get_attr(e, "ExecutionHierarchy")),
                });
            }
            "ProgramUnit" => {
                let unit_type_str = get_attr(e, "ProgramUnitType").unwrap_or_default();
                let unit_type = ProgramUnitType::from_forms(&unit_type_str);
                let raw_text = get_attr(e, "ProgramUnitText").unwrap_or_default();
                let body = decode_double_escaped(&raw_text);
                let text = body.text;
                let unit_name = name.clone().unwrap_or_default();
                let type_name = format!("{:?}", unit_type).to_ascii_uppercase();
                let text_ref = sidecars.write(
                    PlsqlSidecars::PROGRAM_UNITS,
                    &format!("{}.{}", unit_name, type_name),
                    &text,
                )?;

                program_units.push(ProgramUnitInfo {
                    name: unit_name,
                    unit_type,
                    line_count: line_count_of(&text),
                    first_line: first_code_line(&text),
                    inherited,
                    text_encoding: body.encoding,
                    text_ref: Some(text_ref),
                    xml_ref: None,
                    source_text: Some(text),
                });
            }
            "AttachedLibrary" => {
                attached_libraries.push(AttachedLibraryInfo {
                    name: name.clone().unwrap_or_default(),
                    library_location: get_attr(e, "LibraryLocation"),
                });
            }
            "LOV" => {
                *current_lov = Some(LovInfo {
                    name: name.clone().unwrap_or_default(),
                    record_group_name: get_attr(e, "RecordGroupName"),
                    column_mappings: Vec::new(),
                });
            }
            "LOVColumnMapping" => {
                if let Some(l) = current_lov.as_mut() {
                    if let Some(ref n) = name {
                        l.column_mappings.push(n.clone());
                    }
                }
            }
            "RecordGroup" => {
                *current_record_group = Some(RecordGroupInfo {
                    name: name.clone().unwrap_or_default(),
                    query_text: get_attr(e, "RecordGroupQuery"),
                    columns: Vec::new(),
                });
            }
            "RecordGroupColumn" => {
                if let Some(rg) = current_record_group.as_mut() {
                    if let Some(ref n) = name {
                        rg.columns.push(n.clone());
                    }
                }
            }
            "Window" => {
                windows.push(WindowInfo {
                    name: name.clone().unwrap_or_default(),
                    title: get_attr(e, "Title"),
                    modal: get_bool_attr(e, "Modal"),
                    width: get_int_attr(e, "Width"),
                    height: get_int_attr(e, "Height"),
                    horizontal_toolbar_canvas_name: get_attr(e, "HorizontalToolbarCanvasName"),
                    vertical_toolbar_canvas_name: get_attr(e, "VerticalToolbarCanvasName"),
                });
            }
            "Alert" => {
                alerts.push(AlertInfo {
                    name: name.clone().unwrap_or_default(),
                    message: get_attr(e, "AlertMessage").or_else(|| get_attr(e, "Message")),
                    title: get_attr(e, "Title"),
                    button1_label: get_attr(e, "Button1Label"),
                    button2_label: get_attr(e, "Button2Label"),
                    button3_label: get_attr(e, "Button3Label"),
                    default_button: get_attr(e, "DefaultAlertButton").and_then(|b| match b.trim() {
                        "Button 1" | "0" => Some(1),
                        "Button 2" | "1" => Some(2),
                        "Button 3" | "2" => Some(3),
                        _ => None,
                    }),
                });
            }
            "ModuleParameter" => {
                parameters.push(ParameterInfo {
                    name: name.clone().unwrap_or_default(),
                    data_type: get_attr(e, "DataType"),
                    default_value: get_attr(e, "DefaultValue"),
                });
            }
            "VisualAttribute" => {
                if current_tab.is_none() {
                    if let Some(ref n) = name {
                        visual_attributes.push(n.clone());
                    }
                }
            }
            "PropertyClass" => {
                if let Some(ref n) = name {
                    property_classes.push(n.clone());
                    property_class_details.push(PropertyClassInfo {
                        name: n.clone(),
                        item: item_dml(e),
                        block: block_dml(e, false),
                        geometry: item_geometry(e),
                        property_class: local_parent_name(e, module_name),
                        inherited,
                    });
                }
            }
            "Editor" => {
                if let Some(ref n) = name {
                    editors.push(n.clone());
                }
            }
            "Menu" => {
                *current_menu = Some(MenuInfo {
                    name: name.clone().unwrap_or_default(),
                    items: Vec::new(),
                });
            }
            "MenuItem" => {
                if let Some(m) = current_menu.as_mut() {
                    let item_name = name.clone().unwrap_or_default();
                    let command = get_attr(e, "CommandText")
                        .or_else(|| get_attr(e, "MenuItemCode"))
                        .map(|s| decode_double_escaped(&s));
                    let command_ref = if let Some(ref cmd) = command {
                        Some(sidecars.write(
                            PlsqlSidecars::MENU_ITEMS,
                            &format!("{}.{}", m.name, item_name),
                            &cmd.text,
                        )?)
                    } else {
                        None
                    };
                    m.items.push(MenuItemInfo {
                        name: item_name,
                        label: get_attr(e, "Label"),
                        command_type: get_attr(e, "CommandType"),
                        sub_menu_name: get_attr(e, "SubMenuName"),
                        menu_item_type: get_attr(e, "MenuItemType"),
                        command_ref,
                        text_encoding: command.map(|c| c.encoding).unwrap_or_default(),
                    });
                }
            }
            "ObjectLibraryTab" => {
                *current_tab = Some(ObjectLibraryTabInfo {
                    name: name.clone().unwrap_or_default(),
                    entries: Vec::new(),
                });
            }
            _ => {}
        }

        if let Some(t) = current_tab.as_mut() {
            if parent_element.as_deref() == Some("ObjectLibraryTab") {
                if let Some(ref n) = name {
                    t.entries.push(ObjectLibraryEntry {
                        name: n.clone(),
                        object_type: Some(element),
                    });
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_end_element(
        end_line: usize,
        xml_path: &str,
        stack: &mut Vec<Frame>,
        object_refs: &mut Vec<ObjectRef>,
        triggers: &mut [TriggerInfo],
        program_units: &mut [ProgramUnitInfo],
        current_block: &mut Option<BlockBuilder>,
        current_item: &mut Option<ItemBuilder>,
        blocks: &mut Vec<BlockInfo>,
        current_canvas: &mut Option<CanvasBuilder>,
        canvases: &mut Vec<CanvasInfo>,
        current_tab_page_name: &mut Option<String>,
        current_lov: &mut Option<LovInfo>,
        lovs: &mut Vec<LovInfo>,
        current_record_group: &mut Option<RecordGroupInfo>,
        record_groups: &mut Vec<RecordGroupInfo>,
        current_menu: &mut Option<MenuInfo>,
        menus: &mut Vec<MenuInfo>,
        current_tab: &mut Option<ObjectLibraryTabInfo>,
        object_library_tabs: &mut Vec<ObjectLibraryTabInfo>,
    ) {
        let frame = match stack.pop() {
            Some(f) => f,
            None => return,
        };
        let final_end_line = end_line.max(frame.start_line);

        if let Some(ref name) = frame.name {
            object_refs.push(ObjectRef {
                object_type: frame.element.clone(),
                name: name.clone(),
                owner_path: frame.owner_path.clone(),
                inherited: frame.inherited.clone(),
                reference: SourceRef::new(xml_path, frame.start_line, final_end_line),
            });
        }

        match frame.element.as_str() {
            "Trigger" => {
                if let Some(last) = triggers.last_mut() {
                    if last.xml_ref.is_none() {
                        last.xml_ref = Some(SourceRef::new(xml_path, frame.start_line, final_end_line));
                    }
                }
            }
            "ProgramUnit" => {
                if let Some(last) = program_units.last_mut() {
                    if last.xml_ref.is_none() {
                        last.xml_ref = Some(SourceRef::new(xml_path, frame.start_line, final_end_line));
                    }
                }
            }
            "Block" => {
                if let Some(b) = current_block.take() {
                    blocks.push(BlockInfo {
                        name: b.name,
                        query_data_source_name: b.query_data_source_name,
                        property_class: b.property_class,
                        items: b.items,
                        trigger_names: b.trigger_names,
                        inherited: b.inherited,
                        source_ref: Some(SourceRef::new(xml_path, frame.start_line, final_end_line)),
                        dml: b.dml,
                        data_source_column_count: b.data_source_column_count,
                        relations: b.relations,
                        records_displayed: b.records_displayed,
                        distance_between_records: b.distance_between_records,
                        record_orientation: b.record_orientation,
                    });
                }
            }
            "Item" => {
                if let Some(i) = current_item.take() {
                    if let Some(b) = current_block.as_mut() {
                        b.items.push(ItemInfo {
                            name: i.name,
                            item_type: i.item_type,
                            data_type: i.data_type,
                            column_name: i.column_name,
                            canvas_name: i.canvas_name,
                            prompt: i.prompt,
                            property_class: i.property_class,
                            visible: i.visible,
                            required: i.required,
                            lov_name: i.lov_name,
                            trigger_names: i.trigger_names,
                            inherited: i.inherited,
                            dml: i.dml,
                            width: i.width,
                            height: i.height,
                            x_position: i.x_position,
                            y_position: i.y_position,
                            tab_page_name: i.tab_page_name,
                            label: i.label,
                            prompt_attachment_edge: i.prompt_attachment_edge,
                            prompt_attachment_offset: i.prompt_attachment_offset,
                            prompt_align: i.prompt_align,
                            items_display: i.items_display,
                            distance_between_records: i.distance_between_records,
                            back_color: i.back_color,
                            fore_color: i.fore_color,
                            enabled: i.enabled,
                            canvas_visible: i.canvas_visible,
                        });
                    }
                }
            }
            "Canvas" => {
                if let Some(c) = current_canvas.take() {
                    canvases.push(CanvasInfo {
                        name: c.name,
                        canvas_type: c.canvas_type,
                        window_name: c.window_name,
                        property_class: c.property_class,
                        raise_on_enter: c.raise_on_enter,
                        width: c.width,
                        height: c.height,
                        viewport_width: c.viewport_width,
                        viewport_height: c.viewport_height,
                        viewport_x_position: c.viewport_x_position,
                        viewport_y_position: c.viewport_y_position,
                        back_color: c.back_color,
                        tab_pages: c.tab_pages,
                    });
                }
            }
            "TabPage" => {
                *current_tab_page_name = None;
            }
            "LOV" => {
                if let Some(l) = current_lov.take() {
                    lovs.push(l);
                }
            }
            "RecordGroup" => {
                if let Some(rg) = current_record_group.take() {
                    record_groups.push(rg);
                }
            }
            "Menu" => {
                if let Some(m) = current_menu.take() {
                    menus.push(m);
                }
            }
            "ObjectLibraryTab" => {
                if let Some(t) = current_tab.take() {
                    object_library_tabs.push(t);
                }
            }
            _ => {}
        }
    }
}

fn get_attr(e: &BytesStart, name: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if a.key.as_ref() == name.as_bytes() {
            return String::from_utf8(a.value.to_vec()).ok();
        }
    }
    None
}

fn get_bool_attr(e: &BytesStart, name: &str) -> Option<bool> {
    let val = get_attr(e, name)?;
    match val.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn get_int_attr(e: &BytesStart, name: &str) -> Option<i32> {
    get_attr(e, name)?.trim().parse::<i32>().ok()
}

fn item_dml(e: &BytesStart) -> Option<ItemDml> {
    let dml = ItemDml {
        database_item: get_bool_attr(e, "DatabaseItem"),
        insert_allowed: get_bool_attr(e, "InsertAllowed"),
        update_allowed: get_bool_attr(e, "UpdateAllowed"),
        update_if_null: get_bool_attr(e, "UpdateIfNull"),
        query_allowed: get_bool_attr(e, "QueryAllowed"),
        enabled: get_bool_attr(e, "Enabled"),
        keyboard_navigable: get_bool_attr(e, "KeyboardNavigable"),
        primary_key: get_bool_attr(e, "PrimaryKey"),
        required: get_bool_attr(e, "Required"),
        maximum_length: get_int_attr(e, "MaximumLength"),
        initial_value: get_attr(e, "InitializeValue"),
        copy_value_from_item: get_attr(e, "CopyValueFromItem"),
    };
    if dml.is_empty() { None } else { Some(dml) }
}

fn item_geometry(e: &BytesStart) -> Option<ItemGeometry> {
    let geom = ItemGeometry {
        width: get_int_attr(e, "Width"),
        height: get_int_attr(e, "Height"),
    };
    if geom.is_empty() { None } else { Some(geom) }
}

fn block_dml(e: &BytesStart, also_recovered: bool) -> Option<BlockDml> {
    let where_clause = get_attr(e, "WhereClause").map(|s| decode_double_escaped(&s));
    let order_by_clause = get_attr(e, "OrderByClause").map(|s| decode_double_escaped(&s));
    let recovered = also_recovered
        || where_clause.as_ref().map(|w| w.encoding == TextEncoding::Recovered).unwrap_or(false)
        || order_by_clause.as_ref().map(|o| o.encoding == TextEncoding::Recovered).unwrap_or(false);

    let dml = BlockDml {
        database_block: get_bool_attr(e, "DatabaseBlock"),
        insert_allowed: get_bool_attr(e, "InsertAllowed"),
        update_allowed: get_bool_attr(e, "UpdateAllowed"),
        delete_allowed: get_bool_attr(e, "DeleteAllowed"),
        query_allowed: get_bool_attr(e, "QueryAllowed"),
        key_mode: get_attr(e, "KeyMode"),
        lock_mode: get_attr(e, "LockMode"),
        dml_data_target_name: get_attr(e, "DMLDataTargetName"),
        where_clause: where_clause.map(|w| w.text),
        order_by_clause: order_by_clause.map(|o| o.text),
        sql_encoding: if recovered { TextEncoding::Recovered } else { TextEncoding::Original },
    };
    if dml.is_empty() { None } else { Some(dml) }
}

fn local_parent_name(e: &BytesStart, module_name: &str) -> Option<String> {
    if get_attr(e, "ParentFilename").is_some() {
        return None;
    }
    if get_attr(e, "SubclassObjectGroup").map(|s| s.eq_ignore_ascii_case("true")).unwrap_or(false) {
        return None;
    }
    if let Some(pm) = get_attr(e, "ParentModule") {
        if !pm.eq_ignore_ascii_case(module_name) {
            return None;
        }
    }
    get_attr(e, "ParentName")
}

fn inheritance_of(
    e: &BytesStart,
    name: Option<&str>,
    owner_ref: Option<&InheritanceRef>,
    module_name: &str,
) -> Option<InheritanceRef> {
    let parent_module = get_attr(e, "ParentModule");
    let parent_filename = get_attr(e, "ParentFilename");
    let parent_name = get_attr(e, "ParentName");
    let parent_type = get_attr(e, "ParentType");
    let object_group = get_attr(e, "SubclassObjectGroup").map(|s| s.eq_ignore_ascii_case("true")).unwrap_or(false);
    let sub_object = get_attr(e, "SubclassSubObject").map(|s| s.eq_ignore_ascii_case("true")).unwrap_or(false);

    let elsewhere = parent_filename.is_some()
        || (parent_module.is_some() && !parent_module.as_ref().unwrap().eq_ignore_ascii_case(module_name));

    if elsewhere && object_group {
        Some(InheritanceRef {
            module: parent_module,
            file: parent_filename,
            name: name.map(|s| s.to_string()),
            owner_path: owner_ref.and_then(|o| join_path(o.owner_path.as_deref(), o.name.as_deref())),
            object_group: parent_name,
            parent_type,
            sub_object: false,
        })
    } else if elsewhere {
        Some(InheritanceRef {
            module: parent_module,
            file: parent_filename,
            name: parent_name.or_else(|| name.map(|s| s.to_string())),
            owner_path: owner_ref.and_then(|o| join_path(o.owner_path.as_deref(), o.name.as_deref())),
            object_group: None,
            parent_type,
            sub_object: false,
        })
    } else if let (true, Some(o)) = (sub_object, owner_ref) {
        Some(InheritanceRef {
            module: o.module.clone(),
            file: o.file.clone(),
            name: name.map(|s| s.to_string()),
            owner_path: join_path(o.owner_path.as_deref(), o.name.as_deref()),
            object_group: o.object_group.clone(),
            parent_type,
            sub_object: true,
        })
    } else if sub_object {
        Some(InheritanceRef {
            parent_type,
            sub_object: true,
            ..Default::default()
        })
    } else {
        None
    }
}

fn join_path(owner_path: Option<&str>, name: Option<&str>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(op) = owner_path {
        if !op.is_empty() {
            parts.push(op);
        }
    }
    if let Some(n) = name {
        if !n.is_empty() {
            parts.push(n);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

fn owner_path_of(stack: &[Frame]) -> Option<String> {
    let names: Vec<&str> = stack
        .iter()
        .filter(|f| !matches!(f.element.as_str(), "Module" | "FormModule" | "MenuModule" | "ObjectLibrary"))
        .filter_map(|f| f.name.as_deref())
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names.join("."))
    }
}
