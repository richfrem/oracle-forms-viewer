use oracle_forms_core::model::*;
use std::fmt::Write;
use std::fs;
use std::path::Path;

pub struct FormsXmlWriter;

impl FormsXmlWriter {
    /// Serializes a ModuleIndex back into Forms2XML XML string.
    pub fn write_string(module: &ModuleIndex) -> String {
        let mut out = String::with_capacity(32 * 1024);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");

        let version = module
            .forms_version
            .as_deref()
            .unwrap_or("12.2.1.19.0");

        let _ = writeln!(
            out,
            "<Module version=\"{}\" xmlns=\"http://xmlns.oracle.com/Forms\">",
            escape_attr(version)
        );

        match module.key.module_type {
            ModuleType::Form => Self::write_form_module(&mut out, module),
            ModuleType::Menu => Self::write_menu_module(&mut out, module),
            ModuleType::ObjectLibrary => Self::write_object_library(&mut out, module),
            _ => Self::write_form_module(&mut out, module),
        }

        out.push_str("</Module>\n");
        out
    }

    /// Serializes a ModuleIndex and saves it to a file.
    pub fn write_to_file(module: &ModuleIndex, path: &Path) -> std::io::Result<()> {
        let content = Self::write_string(module);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }

    fn write_form_module(out: &mut String, module: &ModuleIndex) {
        let form_name = &module.key.name;
        let mut attrs = vec![format!("Name=\"{}\"", escape_attr(form_name))];

        if let Some(title) = module.windows.iter().find_map(|w| w.title.as_deref()) {
            attrs.push(format!("Title=\"{}\"", escape_attr(title)));
        }
        if let Some(first_win) = module.windows.first() {
            attrs.push(format!("ConsoleWindow=\"{}\"", escape_attr(&first_win.name)));
        }
        if let Some(menu) = &module.menu_module {
            attrs.push(format!("MenuModule=\"{}\"", escape_attr(menu)));
        }

        let _ = writeln!(out, "  <FormModule {}>", attrs.join(" "));

        // Coordinate Info
        let coord = &module.coordinate;
        let mut coord_attrs = Vec::new();
        if let Some(cs) = &coord.coordinate_system {
            coord_attrs.push(format!("CoordinateSystem=\"{}\"", escape_attr(cs)));
        }
        if let Some(ru) = &coord.real_unit {
            coord_attrs.push(format!("RealUnit=\"{}\"", escape_attr(ru)));
        }
        coord_attrs.push(format!("CharacterCellWidth=\"{}\"", coord.character_cell_width));
        coord_attrs.push(format!("CharacterCellHeight=\"{}\"", coord.character_cell_height));
        coord_attrs.push(format!("DefaultFontScaling=\"{}\"", coord.default_font_scaling));
        let _ = writeln!(out, "    <Coordinate {}/>", coord_attrs.join(" "));

        // Alerts
        for alert in &module.alerts {
            let mut a_attrs = vec![format!("Name=\"{}\"", escape_attr(&alert.name))];
            if let Some(msg) = &alert.message {
                a_attrs.push(format!("AlertMessage=\"{}\"", escape_attr(msg)));
            }
            let _ = writeln!(out, "    <Alert {}/>", a_attrs.join(" "));
        }

        // Attached Libraries
        for lib in &module.attached_libraries {
            let mut l_attrs = vec![format!("Name=\"{}\"", escape_attr(&lib.name))];
            if let Some(loc) = &lib.library_location {
                l_attrs.push(format!("LibraryLocation=\"{}\"", escape_attr(loc)));
            }
            let _ = writeln!(out, "    <AttachedLibrary {}/>", l_attrs.join(" "));
        }

        // Blocks
        for block in &module.blocks {
            Self::write_block(out, block, module);
        }

        // Canvases
        for canvas in &module.canvases {
            Self::write_canvas(out, canvas, module);
        }

        // Windows
        for window in &module.windows {
            let mut w_attrs = vec![format!("Name=\"{}\"", escape_attr(&window.name))];
            if let Some(title) = &window.title {
                w_attrs.push(format!("Title=\"{}\"", escape_attr(title)));
            }
            if let Some(modal) = window.modal {
                w_attrs.push(format!("Modal=\"{}\"", modal));
            }
            if let Some(w) = window.width {
                w_attrs.push(format!("Width=\"{}\"", w));
            }
            if let Some(h) = window.height {
                w_attrs.push(format!("Height=\"{}\"", h));
            }
            if let Some(tc) = &window.horizontal_toolbar_canvas_name {
                w_attrs.push(format!("HorizontalToolbarCanvasName=\"{}\"", escape_attr(tc)));
            }
            if let Some(vc) = &window.vertical_toolbar_canvas_name {
                w_attrs.push(format!("VerticalToolbarCanvasName=\"{}\"", escape_attr(vc)));
            }
            let _ = writeln!(out, "    <Window {}/>", w_attrs.join(" "));
        }

        // LOVs & RecordGroups
        for rg in &module.record_groups {
            let mut rg_attrs = vec![format!("Name=\"{}\"", escape_attr(&rg.name))];
            if let Some(q) = &rg.query_text {
                rg_attrs.push(format!("RecordGroupType=\"Query\" RecordGroupQuery=\"{}\"", escape_attr(q)));
            }
            let _ = writeln!(out, "    <RecordGroup {}>", rg_attrs.join(" "));
            for col in &rg.columns {
                let _ = writeln!(out, "      <RecordGroupColumn Name=\"{}\" ColumnDataType=\"Character\"/>", escape_attr(col));
            }
            out.push_str("    </RecordGroup>\n");
        }

        for lov in &module.lovs {
            let mut l_attrs = vec![format!("Name=\"{}\"", escape_attr(&lov.name))];
            if let Some(rg) = &lov.record_group_name {
                l_attrs.push(format!("RecordGroupName=\"{}\"", escape_attr(rg)));
            }
            let _ = writeln!(out, "    <LOV {}>", l_attrs.join(" "));
            for col_map in &lov.column_mappings {
                let _ = writeln!(out, "      <LOVColumnMapping Name=\"{}\"/>", escape_attr(col_map));
            }
            out.push_str("    </LOV>\n");
        }

        // Module Parameters
        for param in &module.parameters {
            let mut p_attrs = vec![format!("Name=\"{}\"", escape_attr(&param.name))];
            if let Some(dt) = &param.data_type {
                p_attrs.push(format!("ParameterDataType=\"{}\"", escape_attr(dt)));
            }
            if let Some(dv) = &param.default_value {
                p_attrs.push(format!("ParameterInitializeValue=\"{}\"", escape_attr(dv)));
            }
            let _ = writeln!(out, "    <ModuleParameter {}/>", p_attrs.join(" "));
        }

        // Program Units
        for pu in &module.program_units {
            let mut pu_attrs = vec![format!("Name=\"{}\"", escape_attr(&pu.name))];
            let p_type_str = match pu.unit_type {
                ProgramUnitType::Procedure => "Procedure",
                ProgramUnitType::Function => "Function",
                ProgramUnitType::PackageSpec => "Package Spec",
                ProgramUnitType::PackageBody => "Package Body",
                _ => "Unknown",
            };
            pu_attrs.push(format!("ProgramUnitType=\"{}\"", p_type_str));
            let text = pu
                .source_text
                .as_deref()
                .unwrap_or(&pu.first_line);
            pu_attrs.push(format!("ProgramUnitText=\"{}\"", escape_attr(text)));
            let _ = writeln!(out, "    <ProgramUnit {}/>", pu_attrs.join(" "));
        }

        // Property Classes
        for pc in &module.property_classes {
            let _ = writeln!(out, "    <PropertyClass Name=\"{}\"/>", escape_attr(pc));
        }

        // Visual Attributes
        for va in &module.visual_attributes {
            let _ = writeln!(out, "    <VisualAttribute Name=\"{}\"/>", escape_attr(va));
        }

        // Editors
        for ed in &module.editors {
            let _ = writeln!(out, "    <Editor Name=\"{}\"/>", escape_attr(ed));
        }

        // Form-level Triggers (no block_name and no item_name)
        for trig in &module.triggers {
            if trig.block_name.is_none() && trig.item_name.is_none() {
                let text = trig
                    .source_text
                    .as_deref()
                    .unwrap_or(&trig.first_line);
                let _ = writeln!(
                    out,
                    "    <Trigger Name=\"{}\" TriggerText=\"{}\"/>",
                    escape_attr(&trig.name),
                    escape_attr(text)
                );
            }
        }

        out.push_str("  </FormModule>\n");
    }

    fn write_block(out: &mut String, block: &BlockInfo, module: &ModuleIndex) {
        let mut b_attrs = vec![format!("Name=\"{}\"", escape_attr(&block.name))];

        if let Some(src) = &block.query_data_source_name {
            b_attrs.push(format!("QueryDataSourceName=\"{}\"", escape_attr(src)));
        }
        if let Some(pc) = &block.property_class {
            b_attrs.push(format!("PropertyClass=\"{}\"", escape_attr(pc)));
        }
        if block.records_displayed > 1 {
            b_attrs.push(format!("RecordsDisplayed=\"{}\"", block.records_displayed));
        }
        if block.distance_between_records > 0 {
            b_attrs.push(format!("DistanceBetweenRecords=\"{}\"", block.distance_between_records));
        }
        if block.record_orientation != "Vertical" && !block.record_orientation.is_empty() {
            b_attrs.push(format!("RecordOrientation=\"{}\"", escape_attr(&block.record_orientation)));
        }

        if let Some(dml) = &block.dml {
            if let Some(where_c) = &dml.where_clause {
                b_attrs.push(format!("WhereClause=\"{}\"", escape_attr(where_c)));
            }
            if let Some(order_c) = &dml.order_by_clause {
                b_attrs.push(format!("OrderByClause=\"{}\"", escape_attr(order_c)));
            }
            if let Some(target) = &dml.dml_data_target_name {
                b_attrs.push(format!("DMLDataTargetName=\"{}\"", escape_attr(target)));
            }
            if let Some(db) = dml.database_block {
                b_attrs.push(format!("DatabaseBlock=\"{}\"", db));
            }
            if let Some(ins) = dml.insert_allowed {
                b_attrs.push(format!("InsertAllowed=\"{}\"", ins));
            }
            if let Some(upd) = dml.update_allowed {
                b_attrs.push(format!("UpdateAllowed=\"{}\"", upd));
            }
            if let Some(del) = dml.delete_allowed {
                b_attrs.push(format!("DeleteAllowed=\"{}\"", del));
            }
            if let Some(qry) = dml.query_allowed {
                b_attrs.push(format!("QueryAllowed=\"{}\"", qry));
            }
            if let Some(lock) = &dml.lock_mode {
                b_attrs.push(format!("LockMode=\"{}\"", escape_attr(lock)));
            }
        }

        let _ = writeln!(out, "    <Block {}>", b_attrs.join(" "));

        // Items
        for item in &block.items {
            Self::write_item(out, &block.name, item, module);
        }

        // Block-level triggers (block_name matches, item_name is None)
        for trig in &module.triggers {
            let is_block_trig = trig
                .block_name
                .as_deref()
                .map(|b| b.eq_ignore_ascii_case(&block.name))
                .unwrap_or(false)
                && trig.item_name.is_none();

            if is_block_trig {
                let text = trig
                    .source_text
                    .as_deref()
                    .unwrap_or(&trig.first_line);
                let _ = writeln!(
                    out,
                    "      <Trigger Name=\"{}\" TriggerText=\"{}\"/>",
                    escape_attr(&trig.name),
                    escape_attr(text)
                );
            }
        }

        // Relations
        for rel in &block.relations {
            let mut r_attrs = vec![format!("Name=\"{}\"", escape_attr(&rel.name))];
            if let Some(det) = &rel.detail_block {
                r_attrs.push(format!("DetailBlock=\"{}\"", escape_attr(det)));
            }
            if let Some(join) = &rel.join_condition {
                r_attrs.push(format!("JoinCondition=\"{}\"", escape_attr(join)));
            }
            if let Some(del) = &rel.delete_record {
                r_attrs.push(format!("DeleteRecord=\"{}\"", escape_attr(del)));
            }
            if let Some(pmo) = rel.prevent_masterless_operations {
                r_attrs.push(format!("PreventMasterlessOperations=\"{}\"", pmo));
            }
            if let Some(aq) = rel.auto_query {
                r_attrs.push(format!("AutoQuery=\"{}\"", aq));
            }
            if let Some(rtype) = &rel.relation_type {
                r_attrs.push(format!("RelationType=\"{}\"", escape_attr(rtype)));
            }
            let _ = writeln!(out, "      <Relation {}/>", r_attrs.join(" "));
        }

        out.push_str("    </Block>\n");
    }

    fn write_item(out: &mut String, block_name: &str, item: &ItemInfo, module: &ModuleIndex) {
        let mut i_attrs = vec![format!("Name=\"{}\"", escape_attr(&item.name))];

        if let Some(itype) = &item.item_type {
            i_attrs.push(format!("ItemType=\"{}\"", escape_attr(itype)));
        }
        if let Some(dtype) = &item.data_type {
            i_attrs.push(format!("DataType=\"{}\"", escape_attr(dtype)));
        }
        if let Some(col) = &item.column_name {
            i_attrs.push(format!("ColumnName=\"{}\"", escape_attr(col)));
        }
        if let Some(canvas) = &item.canvas_name {
            i_attrs.push(format!("CanvasName=\"{}\"", escape_attr(canvas)));
        }
        if let Some(tab) = &item.tab_page_name {
            i_attrs.push(format!("TabPageName=\"{}\"", escape_attr(tab)));
        }
        if let Some(prompt) = &item.prompt {
            i_attrs.push(format!("Prompt=\"{}\"", escape_attr(prompt)));
        }
        if let Some(label) = &item.label {
            i_attrs.push(format!("Label=\"{}\"", escape_attr(label)));
        }
        if let Some(lov) = &item.lov_name {
            i_attrs.push(format!("LOVName=\"{}\"", escape_attr(lov)));
        }
        if let Some(pc) = &item.property_class {
            i_attrs.push(format!("PropertyClass=\"{}\"", escape_attr(pc)));
        }

        // Coordinates & Dimensions
        if item.x_position != 0 {
            i_attrs.push(format!("XPosition=\"{}\"", item.x_position));
        }
        if item.y_position != 0 {
            i_attrs.push(format!("YPosition=\"{}\"", item.y_position));
        }
        if let Some(w) = item.width {
            i_attrs.push(format!("Width=\"{}\"", w));
        }
        if let Some(h) = item.height {
            i_attrs.push(format!("Height=\"{}\"", h));
        }
        if item.visible == Some(false) {
            i_attrs.push("Visible=\"false\"".to_string());
        }
        if !item.enabled {
            i_attrs.push("Enabled=\"false\"".to_string());
        }
        if let Some(bc) = &item.back_color {
            i_attrs.push(format!("BackColor=\"{}\"", escape_attr(bc)));
        }
        if let Some(fc) = &item.fore_color {
            i_attrs.push(format!("ForegroundColor=\"{}\"", escape_attr(fc)));
        }

        // Item DML properties
        if let Some(dml) = &item.dml {
            if let Some(db_item) = dml.database_item {
                i_attrs.push(format!("DatabaseItem=\"{}\"", db_item));
            }
            if let Some(ins) = dml.insert_allowed {
                i_attrs.push(format!("InsertAllowed=\"{}\"", ins));
            }
            if let Some(upd) = dml.update_allowed {
                i_attrs.push(format!("UpdateAllowed=\"{}\"", upd));
            }
            if let Some(qry) = dml.query_allowed {
                i_attrs.push(format!("QueryAllowed=\"{}\"", qry));
            }
            if let Some(req) = dml.required {
                i_attrs.push(format!("Required=\"{}\"", req));
            }
            if let Some(pk) = dml.primary_key {
                i_attrs.push(format!("PrimaryKey=\"{}\"", pk));
            }
            if let Some(max_len) = dml.maximum_length {
                i_attrs.push(format!("MaximumLength=\"{}\"", max_len));
            }
        }

        // Find item-level triggers
        let item_trigs: Vec<&TriggerInfo> = module
            .triggers
            .iter()
            .filter(|t| {
                t.block_name
                    .as_deref()
                    .map(|b| b.eq_ignore_ascii_case(block_name))
                    .unwrap_or(false)
                    && t.item_name
                        .as_deref()
                        .map(|i| i.eq_ignore_ascii_case(&item.name))
                        .unwrap_or(false)
            })
            .collect();

        if item_trigs.is_empty() {
            let _ = writeln!(out, "      <Item {}/>", i_attrs.join(" "));
        } else {
            let _ = writeln!(out, "      <Item {}>", i_attrs.join(" "));
            for trig in item_trigs {
                let text = trig
                    .source_text
                    .as_deref()
                    .unwrap_or(&trig.first_line);
                let _ = writeln!(
                    out,
                    "        <Trigger Name=\"{}\" TriggerText=\"{}\"/>",
                    escape_attr(&trig.name),
                    escape_attr(text)
                );
            }
            out.push_str("      </Item>\n");
        }
    }

    fn write_canvas(out: &mut String, canvas: &CanvasInfo, module: &ModuleIndex) {
        let mut c_attrs = vec![format!("Name=\"{}\"", escape_attr(&canvas.name))];
        if let Some(ctype) = &canvas.canvas_type {
            c_attrs.push(format!("CanvasType=\"{}\"", escape_attr(ctype)));
        }
        if let Some(wname) = &canvas.window_name {
            c_attrs.push(format!("WindowName=\"{}\"", escape_attr(wname)));
        }
        c_attrs.push(format!("Width=\"{}\"", canvas.width));
        c_attrs.push(format!("Height=\"{}\"", canvas.height));
        if let Some(vw) = canvas.viewport_width {
            c_attrs.push(format!("ViewportWidth=\"{}\"", vw));
        }
        if let Some(vh) = canvas.viewport_height {
            c_attrs.push(format!("ViewportHeight=\"{}\"", vh));
        }

        // Graphics / Frames on this canvas
        let canvas_graphics: Vec<&GraphicInfo> = module
            .graphics
            .iter()
            .filter(|g| g.canvas_name.as_deref().map(|c| c.eq_ignore_ascii_case(&canvas.name)).unwrap_or(false))
            .collect();

        if canvas_graphics.is_empty() {
            let _ = writeln!(out, "    <Canvas {}/>", c_attrs.join(" "));
        } else {
            let _ = writeln!(out, "    <Canvas {}>", c_attrs.join(" "));
            for g in canvas_graphics {
                let mut g_attrs = vec![format!("Name=\"{}\"", escape_attr(&g.name))];
                if !g.graphics_type.is_empty() {
                    g_attrs.push(format!("GraphicsType=\"{}\"", escape_attr(&g.graphics_type)));
                }
                if let Some(ftitle) = &g.frame_title {
                    g_attrs.push(format!("FrameTitle=\"{}\"", escape_attr(ftitle)));
                }
                if g.x_position != 0 {
                    g_attrs.push(format!("XPosition=\"{}\"", g.x_position));
                }
                if g.y_position != 0 {
                    g_attrs.push(format!("YPosition=\"{}\"", g.y_position));
                }
                if g.width != 0 {
                    g_attrs.push(format!("Width=\"{}\"", g.width));
                }
                if g.height != 0 {
                    g_attrs.push(format!("Height=\"{}\"", g.height));
                }
                let _ = writeln!(out, "      <Graphics {}/>", g_attrs.join(" "));
            }
            out.push_str("    </Canvas>\n");
        }
    }

    fn write_menu_module(out: &mut String, module: &ModuleIndex) {
        let menu_name = &module.key.name;
        let _ = writeln!(out, "  <MenuModule Name=\"{}\">", escape_attr(menu_name));
        for menu in &module.menus {
            let _ = writeln!(out, "    <Menu Name=\"{}\">", escape_attr(&menu.name));
            for item in &menu.items {
                let mut mi_attrs = vec![format!("Name=\"{}\"", escape_attr(&item.name))];
                if let Some(lbl) = &item.label {
                    mi_attrs.push(format!("Label=\"{}\"", escape_attr(lbl)));
                }
                if let Some(cmd) = &item.command_type {
                    mi_attrs.push(format!("MenuItemType=\"{}\"", escape_attr(cmd)));
                }
                let _ = writeln!(out, "      <MenuItem {}/>", mi_attrs.join(" "));
            }
            out.push_str("    </Menu>\n");
        }
        out.push_str("  </MenuModule>\n");
    }

    fn write_object_library(out: &mut String, module: &ModuleIndex) {
        let olb_name = &module.key.name;
        let _ = writeln!(out, "  <ObjectLibrary Name=\"{}\">", escape_attr(olb_name));
        for tab in &module.object_library_tabs {
            let _ = writeln!(out, "    <ObjectLibraryTab Name=\"{}\">", escape_attr(&tab.name));
            for entry in &tab.entries {
                let mut e_attrs = vec![format!("Name=\"{}\"", escape_attr(&entry.name))];
                if let Some(otype) = &entry.object_type {
                    e_attrs.push(format!("ObjectType=\"{}\"", escape_attr(otype)));
                }
                let _ = writeln!(out, "      <ObjectLibraryEntry {}/>", e_attrs.join(" "));
            }
            out.push_str("    </ObjectLibraryTab>\n");
        }
        out.push_str("  </ObjectLibrary>\n");
    }
}

/// Escapes standard XML characters for attribute strings.
fn escape_attr(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\n' => escaped.push_str("&#10;"),
            '\r' => escaped.push_str("&#13;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
