use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::Path;
use regex::Regex;
use oracle_forms_core::error::Result;
use oracle_forms_core::model::*;
use oracle_forms_core::sidecars::{
    fingerprint_file, first_code_line, line_count_of, PlsqlSidecars,
};
use crate::layout::generate_2column_layout;

pub struct BinaryFmbParser;

impl BinaryFmbParser {
    pub fn parse(key: &ModuleKey, source_file: &Path, module_cache_dir: &Path) -> Result<ModuleIndex> {
        let bytes = fs::read(source_file)?;
        let mut sidecars = PlsqlSidecars::new(module_cache_dir);

        // 1. Detect Forms Version Header from first 16 bytes
        let header_prefix = if bytes.len() >= 16 {
            String::from_utf8_lossy(&bytes[0..16]).to_string()
        } else {
            String::new()
        };
        let forms_version = if header_prefix.starts_with("ROS.60050") {
            "6i (ROS.60050)".to_string()
        } else if header_prefix.starts_with("ROS.") {
            header_prefix.trim_end_matches('\0').trim().to_string()
        } else {
            "Binary .FMB".to_string()
        };

        // Convert whole file bytes into Latin-1 string
        let text_latin1: String = bytes.iter().map(|&b| b as char).collect();

        // 2. Extract Program Units
        let prog_unit_rx = Regex::new(r"(?i)\b(PROCEDURE|FUNCTION|PACKAGE\s+BODY|PACKAGE)\s+([A-Za-z0-9_$]+)\b").unwrap();
        let mut program_units = Vec::new();
        let mut seen_units = HashSet::new();

        let ignore_keywords: HashSet<&str> = [
            "IS", "AS", "BEGIN", "NULL", "THEN", "ELSE", "RETURN"
        ].into_iter().collect();

        for cap in prog_unit_rx.captures_iter(&text_latin1) {
            let m = cap.get(0).unwrap();
            let kind_str = cap.get(1).unwrap().as_str().to_ascii_uppercase();
            let unit_name = cap.get(2).unwrap().as_str().to_ascii_uppercase();

            if seen_units.contains(&unit_name) || ignore_keywords.contains(unit_name.as_str()) {
                continue;
            }

            let start_pos = m.start();
            let chunk = safe_slice(&text_latin1, start_pos, start_pos + 5000);
            let clean_code = extract_complete_code_block(chunk);

            if clean_code.len() > 25 && is_readable_text(&clean_code) {
                seen_units.insert(unit_name.clone());
                let unit_type = ProgramUnitType::from_forms(&kind_str);
                let type_name = format!("{:?}", unit_type).to_ascii_uppercase();
                let text_ref = sidecars.write(
                    PlsqlSidecars::PROGRAM_UNITS,
                    &format!("{}.{}", unit_name, type_name),
                    &clean_code,
                )?;

                program_units.push(ProgramUnitInfo {
                    name: unit_name,
                    unit_type,
                    line_count: line_count_of(&clean_code),
                    first_line: first_code_line(&clean_code),
                    inherited: None,
                    text_encoding: TextEncoding::Original,
                    text_ref: Some(text_ref),
                    xml_ref: None,
                });
            }
        }

        // 3. Extract Triggers (Scoped: TRIGGER_NAME (BLOCK.ITEM))
        let trigger_scope_rx = Regex::new(r"(?i)((?:WHEN-|KEY-|ON-|PRE-|POST-)[A-Z0-9_-]+)\s*\(([A-Z0-9_$.]+)\)").unwrap();
        let mut triggers = Vec::new();
        let mut seen_triggers = HashSet::new();
        let mut discovered_items_by_block: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for cap in trigger_scope_rx.captures_iter(&text_latin1) {
            let m = cap.get(0).unwrap();
            let t_name = cap.get(1).unwrap().as_str().to_ascii_uppercase();
            let scope = cap.get(2).unwrap().as_str().to_ascii_uppercase();
            let tag_pos = m.start();

            // Look up to 1500 chars before tag for the trigger body
            let chunk_before = safe_slice(&text_latin1, tag_pos.saturating_sub(1500), tag_pos);
            let code = find_code_in_chunk(chunk_before);

            let sig = format!("{}.{}", scope, t_name);
            if !seen_triggers.contains(&sig) && !code.trim().is_empty() {
                seen_triggers.insert(sig);

                let (level, b_name, i_name) = if scope.contains('.') {
                    let parts: Vec<&str> = scope.splitn(2, '.').collect();
                    let b = parts[0].to_string();
                    let i = parts[1].to_string();
                    discovered_items_by_block.entry(b.clone()).or_default().insert(i.clone());
                    (TriggerLevel::Item, Some(b), Some(i))
                } else if scope.starts_with("CLS") || scope.contains("MODULE") || scope == key.name {
                    (TriggerLevel::Form, None, None)
                } else {
                    let b = scope.clone();
                    discovered_items_by_block.entry(b.clone()).or_default();
                    (TriggerLevel::Block, Some(b), None)
                };

                let text_ref = sidecars.write(
                    PlsqlSidecars::TRIGGERS,
                    &format!("{}.{}", scope, t_name),
                    &code,
                )?;

                triggers.push(TriggerInfo {
                    name: t_name,
                    level,
                    block_name: b_name,
                    item_name: i_name,
                    first_line: first_code_line(&code),
                    line_count: line_count_of(&code),
                    inherited: None,
                    text_encoding: TextEncoding::Original,
                    text_ref: Some(text_ref),
                    xml_ref: None,
                });
            }
        }

        // 4. Extract Standalone Form-Level Triggers
        let common_form_triggers = [
            ("WHEN-NEW-FORM-INSTANCE", vec!["MMMWhenNewFormInstance", "WhenNewFormInstance"]),
            ("ON-MESSAGE", vec!["MMMOnMessage", "FormOnMessage"]),
            ("ON-ERROR", vec!["MMMOnError", "FormOnError"]),
            ("WHEN-WINDOW-ACTIVATED", vec!["MMMWhenWindowActivated"]),
            ("KEY-EXIT", vec!["MMMKeyExit", "exit_form"]),
        ];

        for (trig_name, keywords) in common_form_triggers {
            if !triggers.iter().any(|t| t.level == TriggerLevel::Form && t.name == trig_name) {
                for kw in keywords {
                    let lower_text = text_latin1.to_ascii_lowercase();
                    if let Some(idx) = lower_text.find(&kw.to_ascii_lowercase()) {
                        let chunk = safe_slice(&text_latin1, idx.saturating_sub(200), idx + 800);
                        let code = find_code_in_chunk(chunk);
                        if code.len() > 20 && is_readable_text(&code) {
                            let text_ref = sidecars.write(
                                PlsqlSidecars::TRIGGERS,
                                &format!("FORM.{}", trig_name),
                                &code,
                            )?;
                            triggers.push(TriggerInfo {
                                name: trig_name.to_string(),
                                level: TriggerLevel::Form,
                                block_name: None,
                                item_name: None,
                                first_line: first_code_line(&code),
                                line_count: line_count_of(&code),
                                inherited: None,
                                text_encoding: TextEncoding::Original,
                                text_ref: Some(text_ref),
                                xml_ref: None,
                            });
                            break;
                        }
                    }
                }
            }
        }

        // 5. Discover Additional Blocks and Items from Bind Variables (:BLOCK.ITEM)
        let bind_rx = Regex::new(r":([A-Za-z0-9_$]+)\.([A-Za-z0-9_$]+)").unwrap();
        for cap in bind_rx.captures_iter(&text_latin1) {
            let b = cap.get(1).unwrap().as_str().to_ascii_uppercase();
            let itm = cap.get(2).unwrap().as_str().to_ascii_uppercase();
            if !matches!(b.as_str(), "GLOBAL" | "SYSTEM" | "PARAMETER" | "V") && (2..=32).contains(&itm.len()) {
                discovered_items_by_block.entry(b).or_default().insert(itm);
            }
        }

        // Also check GO_BLOCK('...')
        let go_block_rx = Regex::new(r"(?i)GO_BLOCK\s*\(\s*[']([A-Za-z0-9_$]+)[']").unwrap();
        for cap in go_block_rx.captures_iter(&text_latin1) {
            let b = cap.get(1).unwrap().as_str().to_ascii_uppercase();
            discovered_items_by_block.entry(b).or_default();
        }

        // 6. Extract Embedded SQL Queries for Record Groups without regex lookahead
        let select_rx = Regex::new(r"(?i)\bSELECT\s+").unwrap();
        let ws_rx = Regex::new(r"\s+").unwrap();
        let mut record_groups = Vec::new();
        let mut query_count = 0;

        for m in select_rx.find_iter(&text_latin1) {
            if query_count >= 20 {
                break;
            }
            let start = m.start();
            let slice = safe_slice(&text_latin1, start, start + 2000);

            // Find delimiter: ';', '\0', or "\n\n"
            let mut end_offset = slice.len();
            if let Some(pos) = slice.find(';') {
                end_offset = end_offset.min(pos + 1);
            }
            if let Some(pos) = slice.find('\0') {
                end_offset = end_offset.min(pos);
            }
            if let Some(pos) = slice.find("\n\n") {
                end_offset = end_offset.min(pos);
            }

            let candidate = &slice[..end_offset];
            if candidate.to_ascii_uppercase().contains(" FROM ") {
                let sql = ws_rx.replace_all(candidate.trim(), " ").to_string();
                if sql.len() > 25 && is_readable_text(&sql) {
                    query_count += 1;
                    let rg_name = format!("RG_QUERY_{}", query_count);
                    record_groups.push(RecordGroupInfo {
                        name: rg_name,
                        query_text: Some(sql),
                        columns: Vec::new(),
                    });
                }
            }
        }

        // 7. Auto-flow 2D Layout
        let layout = generate_2column_layout(&key.name, &discovered_items_by_block, &text_latin1);

        let fingerprint = fingerprint_file(source_file).unwrap_or_default();
        let file_stem = source_file.file_name().and_then(|n| n.to_str()).unwrap_or("module.fmb");
        let converted_file = format!("direct-binary://{}", file_stem);

        use std::time::SystemTime;
        let now = SystemTime::now();
        let dur = now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
        let parsed_at = format!("{}Z", dur.as_secs());

        Ok(ModuleIndex {
            key: key.clone(),
            forms_version: Some(forms_version),
            index_version: oracle_forms_core::CURRENT_INDEX_VERSION,
            source_file: source_file.to_string_lossy().to_string(),
            fingerprint,
            converted_file,
            parsed_at,
            coordinate: CoordinateInfo::default(),
            blocks: layout.blocks,
            triggers,
            program_units,
            attached_libraries: Vec::new(),
            lovs: Vec::new(),
            record_groups,
            windows: layout.windows,
            canvases: layout.canvases,
            graphics: layout.graphics,
            alerts: Vec::new(),
            parameters: Vec::new(),
            visual_attributes: Vec::new(),
            property_classes: Vec::new(),
            property_class_details: Vec::new(),
            editors: Vec::new(),
            menu_module: None,
            menus: Vec::new(),
            object_library_tabs: Vec::new(),
            object_refs: Vec::new(),
        })
    }
}

fn safe_slice(s: &str, mut start: usize, mut end: usize) -> &str {
    start = start.min(s.len());
    end = end.min(s.len()).max(start);
    while start > 0 && !s.is_char_boundary(start) {
        start -= 1;
    }
    while end < s.len() && !s.is_char_boundary(end) {
        end += 1;
    }
    &s[start..end]
}

fn find_code_in_chunk(chunk: &str) -> String {
    let plsql_block_rx = Regex::new(r"(?is)\b(DECLARE|BEGIN)\b.*?\bEND\s*;").unwrap();
    let mut last_match = "";
    for m in plsql_block_rx.find_iter(chunk) {
        last_match = m.as_str().trim();
    }
    if !last_match.is_empty() {
        return clean_code_string(last_match);
    }

    // Fallback: search for lines containing semicolons
    let lines: Vec<&str> = chunk
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && l.contains(';'))
        .collect();

    if !lines.is_empty() {
        lines.join("\n")
    } else {
        String::new()
    }
}

fn extract_complete_code_block(chunk: &str) -> String {
    let upper = chunk.to_ascii_uppercase();
    if let Some(end_idx) = upper.find("END;") {
        let candidate = &chunk[0..end_idx + 4];
        return clean_code_string(candidate.trim());
    }

    let end_named_rx = Regex::new(r"(?i)\bEND\s+[A-Za-z0-9_$]+\s*;").unwrap();
    if let Some(m) = end_named_rx.find(chunk) {
        return clean_code_string(chunk[0..m.end()].trim());
    }

    let limit = 300.min(chunk.len());
    let sub = safe_slice(chunk, 0, limit);
    clean_code_string(sub)
}

fn clean_code_string(raw: &str) -> String {
    let ctrl_re_start = Regex::new(r"^[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\xff]+").unwrap();
    let ctrl_re_end = Regex::new(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\xff]+$").unwrap();

    raw.lines()
        .map(|l| {
            let s1 = ctrl_re_start.replace(l, "");
            ctrl_re_end.replace(&s1, "").to_string()
        })
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_readable_text(s: &str) -> bool {
    if s.trim().is_empty() {
        return false;
    }
    let printable = s.chars().filter(|&c| (' '..='~').contains(&c) || c == '\n' || c == '\r' || c == '\t').count();
    (printable as f64 / s.len() as f64) > 0.90
}
