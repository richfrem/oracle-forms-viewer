use std::collections::{BTreeMap, BTreeSet};
use oracle_forms_core::model::{BlockInfo, CanvasInfo, GraphicInfo, ItemInfo, WindowInfo};

pub struct GeneratedLayout {
    pub blocks: Vec<BlockInfo>,
    pub canvases: Vec<CanvasInfo>,
    pub windows: Vec<WindowInfo>,
    pub graphics: Vec<GraphicInfo>,
}

pub fn generate_2column_layout(
    module_name: &str,
    discovered_items: &BTreeMap<String, BTreeSet<String>>,
    raw_text: &str,
) -> GeneratedLayout {
    let canvas_name = if raw_text.to_ascii_uppercase().contains("CRCCMAINMENU") {
        "CRCCMAINMENU".to_string()
    } else {
        "CV_MAIN".to_string()
    };
    let window_name = "WINDOW_MAIN".to_string();

    let mut blocks = Vec::new();
    let mut graphics = Vec::new();

    let col1_x = 30;
    let col2_x = 420;
    let frame_width = 360;
    let mut col1_y = 30;
    let mut col2_y = 30;
    let canvas_width = 840;

    for (block_name, items_set) in discovered_items {
        if matches!(block_name.as_str(), "GLOBAL" | "SYSTEM" | "PARAMETER") || items_set.is_empty() {
            continue;
        }

        // Place block in the column with lower Y position
        let (frame_x, frame_y, is_col1) = if col1_y <= col2_y {
            (col1_x, col1_y, true)
        } else {
            (col2_x, col2_y, false)
        };

        let mut item_y = frame_y + 32;
        let mut block_items = Vec::new();

        for item_name in items_set {
            let is_button = item_name.starts_with("BTN") || item_name.starts_with("ITSLIB");
            let is_check = item_name.starts_with("CHK") || item_name.contains("CHECK");
            let is_display = item_name.starts_with("DSP") || item_name.starts_with("TXT");

            let (item_type, label, prompt, item_width, item_height, step_y) = if is_button {
                ("Push Button".to_string(), Some(prettify_name(item_name)), None, 160, 28, 36)
            } else if is_check {
                ("Check Box".to_string(), Some(prettify_name(item_name)), None, 140, 22, 30)
            } else if is_display {
                ("Display Item".to_string(), None, Some(prettify_name(item_name)), 140, 22, 30)
            } else {
                ("Text Item".to_string(), None, Some(prettify_name(item_name)), 140, 22, 30)
            };

            let item_x = frame_x + if prompt.is_some() { 120 } else { 20 };

            block_items.push(ItemInfo {
                name: item_name.clone(),
                item_type: Some(item_type),
                data_type: None,
                column_name: None,
                canvas_name: Some(canvas_name.clone()),
                prompt,
                property_class: None,
                visible: Some(true),
                required: None,
                lov_name: None,
                trigger_names: Vec::new(),
                inherited: None,
                dml: None,
                width: Some(item_width),
                height: Some(item_height),
                x_position: item_x,
                y_position: item_y,
                tab_page_name: None,
                label,
                prompt_attachment_edge: "Start".to_string(),
                prompt_attachment_offset: 5,
                prompt_align: "Start".to_string(),
                items_display: 1,
                distance_between_records: 0,
                back_color: None,
                fore_color: None,
                enabled: true,
                canvas_visible: true,
            });

            item_y += step_y;
        }

        let frame_height = (item_y - frame_y + 14).max(70);

        graphics.push(GraphicInfo {
            name: format!("FRAME_{}", block_name),
            canvas_name: Some(canvas_name.clone()),
            tab_page_name: None,
            graphics_type: "Frame".to_string(),
            graphics_text: None,
            frame_title: Some(block_name.clone()),
            x_position: frame_x,
            y_position: frame_y,
            width: frame_width,
            height: frame_height,
            bevel: None,
            back_color: None,
        });

        blocks.push(BlockInfo {
            name: block_name.clone(),
            query_data_source_name: Some(block_name.clone()),
            property_class: None,
            items: block_items,
            trigger_names: Vec::new(),
            inherited: None,
            source_ref: None,
            dml: None,
            data_source_column_count: 0,
            relations: Vec::new(),
            records_displayed: 1,
            distance_between_records: 0,
            record_orientation: "Vertical".to_string(),
        });

        if is_col1 {
            col1_y += frame_height + 25;
        } else {
            col2_y += frame_height + 25;
        }
    }

    let max_canvas_height = col1_y.max(col2_y).max(600) + 40;

    let canvas = CanvasInfo {
        name: canvas_name,
        canvas_type: Some("Content".to_string()),
        window_name: Some(window_name.clone()),
        property_class: None,
        raise_on_enter: None,
        width: canvas_width,
        height: max_canvas_height,
        viewport_width: None,
        viewport_height: None,
        viewport_x_position: 0,
        viewport_y_position: 0,
        back_color: None,
        tab_pages: Vec::new(),
    };

    let window = WindowInfo {
        name: window_name,
        title: Some(format!("{} (Direct Binary .FMB Mode)", module_name)),
        modal: None,
        width: Some(canvas_width),
        height: Some(max_canvas_height),
        horizontal_toolbar_canvas_name: None,
        vertical_toolbar_canvas_name: None,
    };

    GeneratedLayout {
        blocks,
        canvases: vec![canvas],
        windows: vec![window],
        graphics,
    }
}

pub fn prettify_name(name: &str) -> String {
    let re = regex::Regex::new(r"^(?i)(BTN_|BTN|ITSLIB|CHK_|CHK|DSP_|TXT_)").unwrap();
    let stripped = re.replace(name, "").replace('_', " ");
    let mut chars = stripped.chars();
    match chars.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
    }
}
