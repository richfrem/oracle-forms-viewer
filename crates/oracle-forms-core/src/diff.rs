use crate::model::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyChange {
    pub property: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

impl PropertyChange {
    pub fn new(property: impl Into<String>, old_val: Option<String>, new_val: Option<String>) -> Self {
        Self {
            property: property.into(),
            old_value: old_val,
            new_value: new_val,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemGeometryDiff {
    pub old_x: i32,
    pub old_y: i32,
    pub old_width: Option<i32>,
    pub old_height: Option<i32>,
    pub new_x: i32,
    pub new_y: i32,
    pub new_width: Option<i32>,
    pub new_height: Option<i32>,
}

impl ItemGeometryDiff {
    pub fn is_changed(&self) -> bool {
        self.old_x != self.new_x
            || self.old_y != self.new_y
            || self.old_width != self.new_width
            || self.old_height != self.new_height
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDiff {
    pub name: String,
    pub geometry_diff: Option<ItemGeometryDiff>,
    pub property_changes: Vec<PropertyChange>,
}

impl ItemDiff {
    pub fn has_changes(&self) -> bool {
        self.geometry_diff.is_some() || !self.property_changes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerDiff {
    pub name: String,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockDiff {
    pub name: String,
    pub property_changes: Vec<PropertyChange>,
    pub items_added: Vec<ItemInfo>,
    pub items_removed: Vec<ItemInfo>,
    pub items_modified: Vec<ItemDiff>,
    pub triggers_added: Vec<String>,
    pub triggers_removed: Vec<String>,
    pub triggers_modified: Vec<TriggerDiff>,
}

impl BlockDiff {
    pub fn has_changes(&self) -> bool {
        !self.property_changes.is_empty()
            || !self.items_added.is_empty()
            || !self.items_removed.is_empty()
            || !self.items_modified.is_empty()
            || !self.triggers_added.is_empty()
            || !self.triggers_removed.is_empty()
            || !self.triggers_modified.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasDiff {
    pub name: String,
    pub property_changes: Vec<PropertyChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowDiff {
    pub name: String,
    pub property_changes: Vec<PropertyChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDiff {
    #[serde(default)]
    pub model_changes: Vec<PropertyChange>,
    pub base_name: String,
    pub modified_name: String,
    pub blocks_added: Vec<String>,
    pub blocks_removed: Vec<String>,
    pub blocks_modified: Vec<BlockDiff>,
    pub canvases_modified: Vec<CanvasDiff>,
    pub windows_modified: Vec<WindowDiff>,
    pub form_triggers_added: Vec<String>,
    pub form_triggers_removed: Vec<String>,
    pub form_triggers_modified: Vec<TriggerDiff>,
}

impl ModuleDiff {
    pub fn has_differences(&self) -> bool {
        !self.model_changes.is_empty()
            || !self.blocks_added.is_empty()
            || !self.blocks_removed.is_empty()
            || self.blocks_modified.iter().any(|b| b.has_changes())
            || self.canvases_modified.iter().any(|c| !c.property_changes.is_empty())
            || self.windows_modified.iter().any(|w| !w.property_changes.is_empty())
            || !self.form_triggers_added.is_empty()
            || !self.form_triggers_removed.is_empty()
            || !self.form_triggers_modified.is_empty()
    }

    pub fn to_text_report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "=== FORM DIFF: {} vs {} ===\n",
            self.base_name, self.modified_name
        ));

        if !self.has_differences() {
            out.push_str("No differences detected between modules.\n");
            return out;
        }

        if !self.blocks_added.is_empty() {
            out.push_str(&format!("Blocks Added: {}\n", self.blocks_added.join(", ")));
        }
        if !self.blocks_removed.is_empty() {
            out.push_str(&format!("Blocks Removed: {}\n", self.blocks_removed.join(", ")));
        }

        for change in &self.model_changes {
            out.push_str(&format!("Model section changed: {}\n", change.property));
        }
        for block in &self.blocks_modified {
            if !block.has_changes() {
                continue;
            }
            out.push_str(&format!("\n[Block: {}]\n", block.name));
            for pc in &block.property_changes {
                out.push_str(&format!(
                    "  * Property '{}' changed: {:?} -> {:?}\n",
                    pc.property, pc.old_value, pc.new_value
                ));
            }
            for item in &block.items_added {
                out.push_str(&format!("  + Item Added: {} ({:?})\n", item.name, item.item_type));
            }
            for item in &block.items_removed {
                out.push_str(&format!("  - Item Removed: {}\n", item.name));
            }
            for item in &block.items_modified {
                out.push_str(&format!("  ~ Item Modified: {}\n", item.name));
                if let Some(geom) = &item.geometry_diff {
                    out.push_str(&format!(
                        "      Position: ({}, {}) -> ({}, {})\n",
                        geom.old_x, geom.old_y, geom.new_x, geom.new_y
                    ));
                    if geom.old_width != geom.new_width || geom.old_height != geom.new_height {
                        out.push_str(&format!(
                            "      Size: ({:?}x{:?}) -> ({:?}x{:?})\n",
                            geom.old_width, geom.old_height, geom.new_width, geom.new_height
                        ));
                    }
                }
                for pc in &item.property_changes {
                    out.push_str(&format!(
                        "      Property '{}': {:?} -> {:?}\n",
                        pc.property, pc.old_value, pc.new_value
                    ));
                }
            }
            for t in &block.triggers_added {
                out.push_str(&format!("  + Trigger Added: {}\n", t));
            }
            for t in &block.triggers_removed {
                out.push_str(&format!("  - Trigger Removed: {}\n", t));
            }
            for t in &block.triggers_modified {
                out.push_str(&format!("  ~ Trigger Modified: {}\n", t.name));
            }
        }

        for c in &self.canvases_modified {
            if !c.property_changes.is_empty() {
                out.push_str(&format!("\n[Canvas: {}]\n", c.name));
                for pc in &c.property_changes {
                    out.push_str(&format!(
                        "  * Property '{}': {:?} -> {:?}\n",
                        pc.property, pc.old_value, pc.new_value
                    ));
                }
            }
        }

        for w in &self.windows_modified {
            if !w.property_changes.is_empty() {
                out.push_str(&format!("\n[Window: {}]\n", w.name));
                for pc in &w.property_changes {
                    out.push_str(&format!(
                        "  * Property '{}': {:?} -> {:?}\n",
                        pc.property, pc.old_value, pc.new_value
                    ));
                }
            }
        }

        if !self.form_triggers_added.is_empty() {
            out.push_str(&format!("\nForm Triggers Added: {}\n", self.form_triggers_added.join(", ")));
        }
        if !self.form_triggers_removed.is_empty() {
            out.push_str(&format!("Form Triggers Removed: {}\n", self.form_triggers_removed.join(", ")));
        }
        for t in &self.form_triggers_modified {
            out.push_str(&format!("Form Trigger Modified: {}\n", t.name));
        }

        out
    }
}

pub struct FormDiffer;

impl FormDiffer {
    pub fn diff(base: &ModuleIndex, modified: &ModuleIndex) -> ModuleDiff {
        let base_name = base.key.name.clone();
        let modified_name = modified.key.name.clone();

        let mut blocks_added = Vec::new();
        let mut blocks_removed = Vec::new();
        let mut blocks_modified = Vec::new();

        // 1. Blocks comparison
        for b_block in &base.blocks {
            if !modified.blocks.iter().any(|m| m.name.eq_ignore_ascii_case(&b_block.name)) {
                blocks_removed.push(b_block.name.clone());
            }
        }

        for m_block in &modified.blocks {
            if let Some(b_block) = base.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(&m_block.name)) {
                let mut prop_changes = Vec::new();
                if b_block.query_data_source_name != m_block.query_data_source_name {
                    prop_changes.push(PropertyChange::new(
                        "queryDataSourceName",
                        b_block.query_data_source_name.clone(),
                        m_block.query_data_source_name.clone(),
                    ));
                }
                let b_where = b_block.dml.as_ref().and_then(|d| d.where_clause.clone());
                let m_where = m_block.dml.as_ref().and_then(|d| d.where_clause.clone());
                if b_where != m_where {
                    prop_changes.push(PropertyChange::new("whereClause", b_where, m_where));
                }
                let b_order = b_block.dml.as_ref().and_then(|d| d.order_by_clause.clone());
                let m_order = m_block.dml.as_ref().and_then(|d| d.order_by_clause.clone());
                if b_order != m_order {
                    prop_changes.push(PropertyChange::new("orderByClause", b_order, m_order));
                }

                // Items comparison within block
                let mut items_added = Vec::new();
                let mut items_removed = Vec::new();
                let mut items_modified = Vec::new();

                for b_item in &b_block.items {
                    if !m_block.items.iter().any(|m| m.name.eq_ignore_ascii_case(&b_item.name)) {
                        items_removed.push(b_item.clone());
                    }
                }

                for m_item in &m_block.items {
                    if let Some(b_item) = b_block.items.iter().find(|b| b.name.eq_ignore_ascii_case(&m_item.name)) {
                        let mut item_props = Vec::new();
                        if b_item.item_type != m_item.item_type {
                            item_props.push(PropertyChange::new(
                                "itemType",
                                b_item.item_type.clone(),
                                m_item.item_type.clone(),
                            ));
                        }
                        if b_item.data_type != m_item.data_type {
                            item_props.push(PropertyChange::new(
                                "dataType",
                                b_item.data_type.clone(),
                                m_item.data_type.clone(),
                            ));
                        }
                        if b_item.prompt != m_item.prompt {
                            item_props.push(PropertyChange::new(
                                "prompt",
                                b_item.prompt.clone(),
                                m_item.prompt.clone(),
                            ));
                        }
                        if b_item.label != m_item.label {
                            item_props.push(PropertyChange::new(
                                "label",
                                b_item.label.clone(),
                                m_item.label.clone(),
                            ));
                        }
                        if b_item.canvas_name != m_item.canvas_name {
                            item_props.push(PropertyChange::new(
                                "canvasName",
                                b_item.canvas_name.clone(),
                                m_item.canvas_name.clone(),
                            ));
                        }
                        if b_item.visible != m_item.visible {
                            item_props.push(PropertyChange::new(
                                "visible",
                                b_item.visible.map(|v| v.to_string()),
                                m_item.visible.map(|v| v.to_string()),
                            ));
                        }
                        if b_item.enabled != m_item.enabled {
                            item_props.push(PropertyChange::new(
                                "enabled",
                                Some(b_item.enabled.to_string()),
                                Some(m_item.enabled.to_string()),
                            ));
                        }

                        let geom_diff = ItemGeometryDiff {
                            old_x: b_item.x_position,
                            old_y: b_item.y_position,
                            old_width: b_item.width,
                            old_height: b_item.height,
                            new_x: m_item.x_position,
                            new_y: m_item.y_position,
                            new_width: m_item.width,
                            new_height: m_item.height,
                        };

                        let has_geom_change = geom_diff.is_changed();
                        let geom_diff_opt = if has_geom_change { Some(geom_diff) } else { None };

                        if geom_diff_opt.is_some() || !item_props.is_empty() {
                            items_modified.push(ItemDiff {
                                name: m_item.name.clone(),
                                geometry_diff: geom_diff_opt,
                                property_changes: item_props,
                            });
                        }
                    } else {
                        items_added.push(m_item.clone());
                    }
                }

                // Triggers within block
                let mut triggers_added = Vec::new();
                let mut triggers_removed = Vec::new();
                let mut triggers_modified = Vec::new();

                let b_triggers: Vec<&TriggerInfo> = base
                    .triggers
                    .iter()
                    .filter(|t| t.block_name.as_deref().map(|b| b.eq_ignore_ascii_case(&b_block.name)).unwrap_or(false))
                    .collect();
                let m_triggers: Vec<&TriggerInfo> = modified
                    .triggers
                    .iter()
                    .filter(|t| t.block_name.as_deref().map(|b| b.eq_ignore_ascii_case(&m_block.name)).unwrap_or(false))
                    .collect();

                for bt in &b_triggers {
                    if !m_triggers.iter().any(|mt| same_trigger(mt, bt)) {
                        triggers_removed.push(bt.name.clone());
                    }
                }
                for mt in &m_triggers {
                    if let Some(bt) = b_triggers.iter().find(|t| same_trigger(t, mt)) {
                        if bt.source_text != mt.source_text || bt.first_line != mt.first_line {
                            triggers_modified.push(TriggerDiff {
                                name: mt.name.clone(),
                                old_text: bt.source_text.clone().or_else(|| Some(bt.first_line.clone())),
                                new_text: mt.source_text.clone().or_else(|| Some(mt.first_line.clone())),
                            });
                        }
                    } else {
                        triggers_added.push(mt.name.clone());
                    }
                }

                let block_diff = BlockDiff {
                    name: m_block.name.clone(),
                    property_changes: prop_changes,
                    items_added,
                    items_removed,
                    items_modified,
                    triggers_added,
                    triggers_removed,
                    triggers_modified,
                };

                if block_diff.has_changes() {
                    blocks_modified.push(block_diff);
                }
            } else {
                blocks_added.push(m_block.name.clone());
            }
        }

        // 2. Canvases comparison
        let mut canvases_modified = Vec::new();
        for m_canvas in &modified.canvases {
            if let Some(b_canvas) = base.canvases.iter().find(|c| c.name.eq_ignore_ascii_case(&m_canvas.name)) {
                let mut prop_changes = Vec::new();
                if b_canvas.width != m_canvas.width {
                    prop_changes.push(PropertyChange::new(
                        "width",
                        Some(b_canvas.width.to_string()),
                        Some(m_canvas.width.to_string()),
                    ));
                }
                if b_canvas.height != m_canvas.height {
                    prop_changes.push(PropertyChange::new(
                        "height",
                        Some(b_canvas.height.to_string()),
                        Some(m_canvas.height.to_string()),
                    ));
                }
                if b_canvas.window_name != m_canvas.window_name {
                    prop_changes.push(PropertyChange::new(
                        "windowName",
                        b_canvas.window_name.clone(),
                        m_canvas.window_name.clone(),
                    ));
                }
                if !prop_changes.is_empty() {
                    canvases_modified.push(CanvasDiff {
                        name: m_canvas.name.clone(),
                        property_changes: prop_changes,
                    });
                }
            }
        }

        // 3. Windows comparison
        let mut windows_modified = Vec::new();
        for m_window in &modified.windows {
            if let Some(b_window) = base.windows.iter().find(|w| w.name.eq_ignore_ascii_case(&m_window.name)) {
                let mut prop_changes = Vec::new();
                if b_window.title != m_window.title {
                    prop_changes.push(PropertyChange::new(
                        "title",
                        b_window.title.clone(),
                        m_window.title.clone(),
                    ));
                }
                if b_window.width != m_window.width {
                    prop_changes.push(PropertyChange::new(
                        "width",
                        b_window.width.map(|w| w.to_string()),
                        m_window.width.map(|w| w.to_string()),
                    ));
                }
                if b_window.height != m_window.height {
                    prop_changes.push(PropertyChange::new(
                        "height",
                        b_window.height.map(|h| h.to_string()),
                        m_window.height.map(|h| h.to_string()),
                    ));
                }
                if !prop_changes.is_empty() {
                    windows_modified.push(WindowDiff {
                        name: m_window.name.clone(),
                        property_changes: prop_changes,
                    });
                }
            }
        }

        // 4. Form Triggers comparison
        let mut form_triggers_added = Vec::new();
        let mut form_triggers_removed = Vec::new();
        let mut form_triggers_modified = Vec::new();

        let b_form_trigs: Vec<&TriggerInfo> = base
            .triggers
            .iter()
            .filter(|t| t.block_name.is_none() && t.item_name.is_none())
            .collect();
        let m_form_trigs: Vec<&TriggerInfo> = modified
            .triggers
            .iter()
            .filter(|t| t.block_name.is_none() && t.item_name.is_none())
            .collect();

        for bt in &b_form_trigs {
            if !m_form_trigs.iter().any(|mt| same_trigger(mt, bt)) {
                form_triggers_removed.push(bt.name.clone());
            }
        }
        for mt in &m_form_trigs {
            if let Some(bt) = b_form_trigs.iter().find(|t| same_trigger(t, mt)) {
                if bt.source_text != mt.source_text || bt.first_line != mt.first_line {
                    form_triggers_modified.push(TriggerDiff {
                        name: mt.name.clone(),
                        old_text: bt.source_text.clone().or_else(|| Some(bt.first_line.clone())),
                        new_text: mt.source_text.clone().or_else(|| Some(mt.first_line.clone())),
                    });
                }
            } else {
                form_triggers_added.push(mt.name.clone());
            }
        }

        // Supplement the focused layout report with every differing parsed model section.
        // Source locations and ingestion metadata do not represent a form behavior change.
        let mut old = serde_json::to_value(base).expect("ModuleIndex serializes");
        let mut new = serde_json::to_value(modified).expect("ModuleIndex serializes");
        strip_locations(&mut old);
        strip_locations(&mut new);
        let mut model_changes = Vec::new();
        for (name, value) in old.as_object().expect("ModuleIndex is an object") {
            if matches!(name.as_str(), "key" | "sourceFile" | "convertedFile" | "parsedAt" | "fingerprint" | "indexVersion") { continue; }
            if new.get(name) != Some(value) {
                model_changes.push(PropertyChange::new(name.clone(), Some(value.to_string()), new.get(name).map(|v| v.to_string())));
            }
        }
        ModuleDiff {
            model_changes,
            base_name,
            modified_name,
            blocks_added,
            blocks_removed,
            blocks_modified,
            canvases_modified,
            windows_modified,
            form_triggers_added,
            form_triggers_removed,
            form_triggers_modified,
        }
    }
}

fn same_trigger(a: &TriggerInfo, b: &TriggerInfo) -> bool {
    fn same(a: Option<&str>, b: Option<&str>) -> bool {
        match (a, b) { (Some(a), Some(b)) => a.eq_ignore_ascii_case(b), (None, None) => true, _ => false }
    }
    a.name.eq_ignore_ascii_case(&b.name) && same(a.block_name.as_deref(), b.block_name.as_deref())
        && same(a.item_name.as_deref(), b.item_name.as_deref())
}

fn strip_locations(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for name in ["sourceRef", "textRef", "xmlRef"] { map.remove(name); }
            for child in map.values_mut() { strip_locations(child); }
        }
        serde_json::Value::Array(values) => { for child in values { strip_locations(child); } }
        _ => {}
    }
}
