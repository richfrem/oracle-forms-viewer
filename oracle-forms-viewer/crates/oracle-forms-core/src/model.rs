use serde::{Deserialize, Serialize};

pub const CURRENT_INDEX_VERSION: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModuleType {
    Form,
    Menu,
    Library,
    ObjectLibrary,
    Report,
}

impl ModuleType {
    pub fn extension(&self) -> &'static str {
        match self {
            ModuleType::Form => "fmb",
            ModuleType::Menu => "mmb",
            ModuleType::Library => "pll",
            ModuleType::ObjectLibrary => "olb",
            ModuleType::Report => "rdf",
        }
    }

    pub fn converted_suffix(&self) -> &'static str {
        match self {
            ModuleType::Form => "_fmb.xml",
            ModuleType::Menu => "_mmb.xml",
            ModuleType::Library => ".pld",
            ModuleType::ObjectLibrary => "_olb.xml",
            ModuleType::Report => "_rdf.xml",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "fmb" => Some(ModuleType::Form),
            "mmb" => Some(ModuleType::Menu),
            "pll" => Some(ModuleType::Library),
            "olb" => Some(ModuleType::ObjectLibrary),
            "pld" => Some(ModuleType::Library),
            "rdf" | "rpt" => Some(ModuleType::Report),
            "xml" => Some(ModuleType::Form),
            _ => None,
        }
    }

    pub fn match_converted(file_name: &str) -> Option<(String, ModuleType)> {
        let lower = file_name.to_ascii_lowercase();
        let types = [
            ModuleType::Form,
            ModuleType::Menu,
            ModuleType::Library,
            ModuleType::ObjectLibrary,
            ModuleType::Report,
        ];
        for t in types {
            let suffix = t.converted_suffix();
            if lower.ends_with(suffix) {
                let base = &file_name[..file_name.len() - suffix.len()];
                if !base.is_empty() {
                    return Some((base.to_ascii_uppercase(), t));
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleKey {
    pub name: String,
    #[serde(rename = "type")]
    pub module_type: ModuleType,
}

impl ModuleKey {
    pub fn new(name: impl Into<String>, module_type: ModuleType) -> Self {
        Self {
            name: name.into().trim().to_ascii_uppercase(),
            module_type,
        }
    }

    pub fn from_file_name(file_name: &str) -> Option<Self> {
        if let Some((base, m_type)) = ModuleType::match_converted(file_name) {
            return Some(Self::new(base, m_type));
        }
        let p = std::path::Path::new(file_name);
        let ext = p.extension()?.to_str()?;
        let stem = p.file_stem()?.to_str()?;
        let m_type = ModuleType::from_extension(ext)?;
        Some(Self::new(stem, m_type))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModuleFingerprint {
    pub size_bytes: u64,
    pub last_modified_millis: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinateInfo {
    pub coordinate_system: Option<String>,
    pub real_unit: Option<String>,
    pub character_cell_width: i32,
    pub character_cell_height: i32,
    pub default_font_scaling: bool,
}

impl Default for CoordinateInfo {
    fn default() -> Self {
        Self {
            coordinate_system: None,
            real_unit: Some("Point".to_string()),
            character_cell_width: 7,
            character_cell_height: 14,
            default_font_scaling: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TextEncoding {
    #[default]
    Original,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl SourceRef {
    pub fn new(file: impl Into<String>, start_line: usize, end_line: usize) -> Self {
        Self {
            file: file.into(),
            start_line,
            end_line,
        }
    }

    pub fn line_count(&self) -> usize {
        if self.end_line >= self.start_line {
            self.end_line - self.start_line + 1
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InheritanceRef {
    pub module: Option<String>,
    pub file: Option<String>,
    pub name: Option<String>,
    pub owner_path: Option<String>,
    pub object_group: Option<String>,
    pub parent_type: Option<String>,
    #[serde(default)]
    pub sub_object: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlockDml {
    pub database_block: Option<bool>,
    pub insert_allowed: Option<bool>,
    pub update_allowed: Option<bool>,
    pub delete_allowed: Option<bool>,
    pub query_allowed: Option<bool>,
    pub key_mode: Option<String>,
    pub lock_mode: Option<String>,
    pub dml_data_target_name: Option<String>,
    pub where_clause: Option<String>,
    pub order_by_clause: Option<String>,
    #[serde(default)]
    pub sql_encoding: TextEncoding,
}

impl BlockDml {
    pub fn is_empty(&self) -> bool {
        self.database_block.is_none()
            && self.insert_allowed.is_none()
            && self.update_allowed.is_none()
            && self.delete_allowed.is_none()
            && self.query_allowed.is_none()
            && self.key_mode.is_none()
            && self.lock_mode.is_none()
            && self.dml_data_target_name.is_none()
            && self.where_clause.is_none()
            && self.order_by_clause.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ItemDml {
    pub database_item: Option<bool>,
    pub insert_allowed: Option<bool>,
    pub update_allowed: Option<bool>,
    pub update_if_null: Option<bool>,
    pub query_allowed: Option<bool>,
    pub enabled: Option<bool>,
    pub keyboard_navigable: Option<bool>,
    pub primary_key: Option<bool>,
    pub required: Option<bool>,
    pub maximum_length: Option<i32>,
    pub initial_value: Option<String>,
    pub copy_value_from_item: Option<String>,
}

impl ItemDml {
    pub fn is_empty(&self) -> bool {
        self.database_item.is_none()
            && self.insert_allowed.is_none()
            && self.update_allowed.is_none()
            && self.update_if_null.is_none()
            && self.query_allowed.is_none()
            && self.enabled.is_none()
            && self.keyboard_navigable.is_none()
            && self.primary_key.is_none()
            && self.required.is_none()
            && self.maximum_length.is_none()
            && self.initial_value.is_none()
            && self.copy_value_from_item.is_none()
    }

    pub fn over(&self, fallback: &ItemDml) -> ItemDml {
        ItemDml {
            database_item: self.database_item.or(fallback.database_item),
            insert_allowed: self.insert_allowed.or(fallback.insert_allowed),
            update_allowed: self.update_allowed.or(fallback.update_allowed),
            update_if_null: self.update_if_null.or(fallback.update_if_null),
            query_allowed: self.query_allowed.or(fallback.query_allowed),
            enabled: self.enabled.or(fallback.enabled),
            keyboard_navigable: self.keyboard_navigable.or(fallback.keyboard_navigable),
            primary_key: self.primary_key.or(fallback.primary_key),
            required: self.required.or(fallback.required),
            maximum_length: self.maximum_length.or(fallback.maximum_length),
            initial_value: self.initial_value.clone().or_else(|| fallback.initial_value.clone()),
            copy_value_from_item: self.copy_value_from_item.clone().or_else(|| fallback.copy_value_from_item.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ItemGeometry {
    pub width: Option<i32>,
    pub height: Option<i32>,
}

impl ItemGeometry {
    pub fn is_empty(&self) -> bool {
        self.width.is_none() && self.height.is_none()
    }

    pub fn over(&self, fallback: &ItemGeometry) -> ItemGeometry {
        ItemGeometry {
            width: self.width.or(fallback.width),
            height: self.height.or(fallback.height),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationInfo {
    pub name: String,
    pub detail_block: Option<String>,
    pub join_condition: Option<String>,
    #[serde(default)]
    pub join_encoding: TextEncoding,
    pub deferred: Option<bool>,
    pub auto_query: Option<bool>,
    pub prevent_masterless_operations: Option<bool>,
    pub delete_record: Option<String>,
    pub relation_type: Option<String>,
    pub inherited: Option<InheritanceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInfo {
    pub name: String,
    pub item_type: Option<String>,
    pub data_type: Option<String>,
    pub column_name: Option<String>,
    pub canvas_name: Option<String>,
    pub prompt: Option<String>,
    pub property_class: Option<String>,
    pub visible: Option<bool>,
    pub required: Option<bool>,
    pub lov_name: Option<String>,
    #[serde(default)]
    pub trigger_names: Vec<String>,
    pub inherited: Option<InheritanceRef>,
    pub dml: Option<ItemDml>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    #[serde(default)]
    pub x_position: i32,
    #[serde(default)]
    pub y_position: i32,
    pub tab_page_name: Option<String>,
    pub label: Option<String>,
    #[serde(default = "default_edge")]
    pub prompt_attachment_edge: String,
    #[serde(default = "default_prompt_offset")]
    pub prompt_attachment_offset: i32,
    #[serde(default = "default_align")]
    pub prompt_align: String,
    #[serde(default = "default_one")]
    pub items_display: i32,
    #[serde(default)]
    pub distance_between_records: i32,
    pub back_color: Option<String>,
    pub fore_color: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub canvas_visible: bool,
}

fn default_edge() -> String { "Start".to_string() }
fn default_prompt_offset() -> i32 { 5 }
fn default_align() -> String { "Start".to_string() }
fn default_one() -> i32 { 1 }
fn default_true() -> bool { true }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockInfo {
    pub name: String,
    pub query_data_source_name: Option<String>,
    pub property_class: Option<String>,
    #[serde(default)]
    pub items: Vec<ItemInfo>,
    #[serde(default)]
    pub trigger_names: Vec<String>,
    pub inherited: Option<InheritanceRef>,
    pub source_ref: Option<SourceRef>,
    pub dml: Option<BlockDml>,
    #[serde(default)]
    pub data_source_column_count: i32,
    #[serde(default)]
    pub relations: Vec<RelationInfo>,
    #[serde(default = "default_one")]
    pub records_displayed: i32,
    #[serde(default)]
    pub distance_between_records: i32,
    #[serde(default = "default_orientation")]
    pub record_orientation: String,
}

fn default_orientation() -> String { "Vertical".to_string() }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabPageInfo {
    pub name: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasInfo {
    pub name: String,
    pub canvas_type: Option<String>,
    pub window_name: Option<String>,
    pub property_class: Option<String>,
    pub raise_on_enter: Option<bool>,
    #[serde(default = "default_canvas_width")]
    pub width: i32,
    #[serde(default = "default_canvas_height")]
    pub height: i32,
    pub viewport_width: Option<i32>,
    pub viewport_height: Option<i32>,
    #[serde(default)]
    pub viewport_x_position: i32,
    #[serde(default)]
    pub viewport_y_position: i32,
    pub back_color: Option<String>,
    #[serde(default)]
    pub tab_pages: Vec<TabPageInfo>,
}

fn default_canvas_width() -> i32 { 800 }
fn default_canvas_height() -> i32 { 600 }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicInfo {
    pub name: String,
    pub canvas_name: Option<String>,
    pub tab_page_name: Option<String>,
    #[serde(default = "default_graphic_type")]
    pub graphics_type: String,
    pub graphics_text: Option<String>,
    pub frame_title: Option<String>,
    #[serde(default)]
    pub x_position: i32,
    #[serde(default)]
    pub y_position: i32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    pub bevel: Option<String>,
    pub back_color: Option<String>,
}

fn default_graphic_type() -> String { "Rectangle".to_string() }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TriggerLevel {
    Form,
    Block,
    Item,
    Menu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInfo {
    pub name: String,
    pub level: TriggerLevel,
    pub block_name: Option<String>,
    pub item_name: Option<String>,
    #[serde(default)]
    pub first_line: String,
    #[serde(default)]
    pub line_count: usize,
    pub inherited: Option<InheritanceRef>,
    #[serde(default)]
    pub text_encoding: TextEncoding,
    pub text_ref: Option<SourceRef>,
    pub xml_ref: Option<SourceRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProgramUnitType {
    Procedure,
    Function,
    PackageSpec,
    PackageBody,
    Unknown,
}

impl ProgramUnitType {
    pub fn from_forms(val: &str) -> Self {
        match val.trim().to_ascii_uppercase().as_str() {
            "PROCEDURE" => ProgramUnitType::Procedure,
            "FUNCTION" => ProgramUnitType::Function,
            "PACKAGE SPEC" | "PACKAGE SPECIFICATION" | "PACKAGE" => ProgramUnitType::PackageSpec,
            "PACKAGE BODY" => ProgramUnitType::PackageBody,
            _ => ProgramUnitType::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramUnitInfo {
    pub name: String,
    #[serde(default = "default_prog_type")]
    pub unit_type: ProgramUnitType,
    #[serde(default)]
    pub line_count: usize,
    #[serde(default)]
    pub first_line: String,
    pub inherited: Option<InheritanceRef>,
    #[serde(default)]
    pub text_encoding: TextEncoding,
    pub text_ref: Option<SourceRef>,
    pub xml_ref: Option<SourceRef>,
}

fn default_prog_type() -> ProgramUnitType { ProgramUnitType::Unknown }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachedLibraryInfo {
    pub name: String,
    pub library_location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LovInfo {
    pub name: String,
    pub record_group_name: Option<String>,
    #[serde(default)]
    pub column_mappings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordGroupInfo {
    pub name: String,
    pub query_text: Option<String>,
    #[serde(default)]
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub name: String,
    pub title: Option<String>,
    pub modal: Option<bool>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub horizontal_toolbar_canvas_name: Option<String>,
    pub vertical_toolbar_canvas_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertInfo {
    pub name: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterInfo {
    pub name: String,
    pub data_type: Option<String>,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyClassInfo {
    pub name: String,
    pub item: Option<ItemDml>,
    pub block: Option<BlockDml>,
    pub geometry: Option<ItemGeometry>,
    pub property_class: Option<String>,
    pub inherited: Option<InheritanceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuItemInfo {
    pub name: String,
    pub label: Option<String>,
    pub command_type: Option<String>,
    #[serde(default)]
    pub sub_menu_name: Option<String>,
    #[serde(default)]
    pub menu_item_type: Option<String>,
    pub command_ref: Option<SourceRef>,
    #[serde(default)]
    pub text_encoding: TextEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuInfo {
    pub name: String,
    #[serde(default)]
    pub items: Vec<MenuItemInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectLibraryEntry {
    pub name: String,
    pub object_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectLibraryTabInfo {
    pub name: String,
    #[serde(default)]
    pub entries: Vec<ObjectLibraryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub object_type: String,
    pub name: String,
    pub owner_path: Option<String>,
    pub inherited: Option<InheritanceRef>,
    #[serde(rename = "ref")]
    pub reference: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleIndex {
    pub key: ModuleKey,
    pub forms_version: Option<String>,
    #[serde(default = "default_index_version")]
    pub index_version: i32,
    pub source_file: String,
    pub fingerprint: ModuleFingerprint,
    pub converted_file: String,
    pub parsed_at: String, // ISO 8601 string timestamp
    #[serde(default)]
    pub coordinate: CoordinateInfo,
    #[serde(default)]
    pub blocks: Vec<BlockInfo>,
    #[serde(default)]
    pub triggers: Vec<TriggerInfo>,
    #[serde(default)]
    pub program_units: Vec<ProgramUnitInfo>,
    #[serde(default)]
    pub attached_libraries: Vec<AttachedLibraryInfo>,
    #[serde(default)]
    pub lovs: Vec<LovInfo>,
    #[serde(default)]
    pub record_groups: Vec<RecordGroupInfo>,
    #[serde(default)]
    pub windows: Vec<WindowInfo>,
    #[serde(default)]
    pub canvases: Vec<CanvasInfo>,
    #[serde(default)]
    pub graphics: Vec<GraphicInfo>,
    #[serde(default)]
    pub alerts: Vec<AlertInfo>,
    #[serde(default)]
    pub parameters: Vec<ParameterInfo>,
    #[serde(default)]
    pub visual_attributes: Vec<String>,
    #[serde(default)]
    pub property_classes: Vec<String>,
    #[serde(default)]
    pub property_class_details: Vec<PropertyClassInfo>,
    #[serde(default)]
    pub editors: Vec<String>,
    #[serde(default)]
    pub menu_module: Option<String>,
    #[serde(default)]
    pub menus: Vec<MenuInfo>,
    #[serde(default)]
    pub object_library_tabs: Vec<ObjectLibraryTabInfo>,
    #[serde(default)]
    pub object_refs: Vec<ObjectRef>,
}

fn default_index_version() -> i32 { CURRENT_INDEX_VERSION }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleSummaryDto {
    pub module_name: String,
    pub module_type: String,
    pub title: Option<String>,
    pub forms_version: Option<String>,
    pub block_count: usize,
    pub item_count: usize,
    pub canvas_count: usize,
    pub window_count: usize,
    pub trigger_count: usize,
    pub program_unit_count: usize,
}

impl ModuleIndex {
    pub fn to_summary(&self) -> ModuleSummaryDto {
        let total_items: usize = self.blocks.iter().map(|b| b.items.len()).sum();
        let title = self.windows.iter().find_map(|w| w.title.clone()).unwrap_or_else(|| self.key.name.clone());
        let type_str = match self.key.module_type {
            ModuleType::Form => "FORM",
            ModuleType::Menu => "MENU",
            ModuleType::Library => "LIBRARY",
            ModuleType::ObjectLibrary => "OBJECT_LIBRARY",
            ModuleType::Report => "REPORT",
        };
        ModuleSummaryDto {
            module_name: self.key.name.clone(),
            module_type: type_str.to_string(),
            title: Some(title),
            forms_version: self.forms_version.clone(),
            block_count: self.blocks.len(),
            item_count: total_items,
            canvas_count: self.canvases.len(),
            window_count: self.windows.len(),
            trigger_count: self.triggers.len(),
            program_unit_count: self.program_units.len(),
        }
    }

    pub fn to_payload(self) -> ModuleViewPayload {
        let summary = self.to_summary();
        ModuleViewPayload {
            summary,
            module_index: self,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleViewPayload {
    pub summary: ModuleSummaryDto,
    pub module_index: ModuleIndex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileRequest {
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub error: String,
    #[serde(default)]
    pub requires_conversion: bool,
    #[serde(default)]
    pub sample_options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeResponse {
    pub module_name: String,
    pub path: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlSliceResponse {
    pub module_name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub xml: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveItemProperties {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub x_position: i32,
    pub y_position: i32,
    pub prompt: Option<String>,
    pub item_type: Option<String>,
    pub dml: Option<ItemDml>,
    pub resolved_through: Vec<String>,
}
