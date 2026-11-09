use std::fs;
use std::path::Path;
use oracle_forms_core::error::Result;
use oracle_forms_core::model::{
    ModuleFingerprint, ModuleIndex, ModuleKey, ProgramUnitInfo, ProgramUnitType, SourceRef,
};
use oracle_forms_core::sidecars::{cache_relative, first_code_line};
use regex::Regex;

pub struct PldParser;

impl PldParser {
    pub fn parse(key: &ModuleKey, pld_file: &Path, module_cache_dir: &Path) -> Result<ModuleIndex> {
        let lines = Self::read_lines(pld_file)?;
        let pld_path = cache_relative(pld_file, module_cache_dir);

        let header_re = Regex::new(r"^(?i)(PACKAGE\s+BODY|PACKAGE|PROCEDURE|FUNCTION)\s+([A-Za-z0-9_$#]+)").unwrap();

        struct UnitHeader {
            line_idx: usize, // 0-based
            kind: String,
            name: String,
        }

        let mut headers = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if let Some(cap) = header_re.captures(line) {
                let kind = cap.get(1).unwrap().as_str().to_string();
                let name = cap.get(2).unwrap().as_str().to_string();
                headers.push(UnitHeader {
                    line_idx: i,
                    kind,
                    name,
                });
            }
        }

        let mut units = Vec::new();
        for (i, header) in headers.iter().enumerate() {
            let next_line = if i + 1 < headers.len() {
                headers[i + 1].line_idx
            } else {
                lines.len()
            };

            let mut end_line_idx = next_line;
            while end_line_idx > header.line_idx && lines[end_line_idx - 1].trim().is_empty() {
                end_line_idx -= 1;
            }

            let start_line = header.line_idx + 1; // 1-based
            let end_line = end_line_idx; // 1-based
            let line_count = if end_line >= start_line {
                end_line - start_line + 1
            } else {
                1
            };

            let kind_normalized = header.kind.split_whitespace().collect::<Vec<_>>().join(" ");
            let unit_type = ProgramUnitType::from_forms(&kind_normalized);
            let first_line = first_code_line(&lines[header.line_idx]);

            units.push(ProgramUnitInfo {
                name: header.name.to_ascii_uppercase(),
                unit_type,
                line_count,
                first_line,
                source_text: Some(lines[header.line_idx..end_line_idx].join("\n")),
                inherited: None,
                text_encoding: Default::default(),
                text_ref: Some(SourceRef::new(&pld_path, start_line, end_line)),
                xml_ref: None,
            });
        }

        let now = chrono_iso_now();

        Ok(ModuleIndex {
            key: key.clone(),
            forms_version: None,
            index_version: oracle_forms_core::CURRENT_INDEX_VERSION,
            source_file: pld_file.to_string_lossy().to_string(),
            fingerprint: ModuleFingerprint::default(),
            converted_file: pld_path,
            parsed_at: now,
            coordinate: Default::default(),
            blocks: Vec::new(),
            triggers: Vec::new(),
            program_units: units,
            attached_libraries: Vec::new(),
            lovs: Vec::new(),
            record_groups: Vec::new(),
            windows: Vec::new(),
            canvases: Vec::new(),
            graphics: Vec::new(),
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

    fn read_lines(file: &Path) -> Result<Vec<String>> {
        let bytes = fs::read(file)?;
        let text = match String::from_utf8(bytes.clone()) {
            Ok(utf8_str) => utf8_str,
            Err(_) => {
                // Windows-1252 / ISO-8859-1 fallback
                bytes.iter().map(|&b| b as char).collect()
            }
        };
        let normalized = text.replace("\r\n", "\n");
        Ok(normalized.split('\n').map(|s| s.to_string()).collect())
    }
}

pub fn chrono_iso_now() -> String {
    // Generate ISO 8601 timestamp string from system time without external chrono crate
    use std::time::SystemTime;
    let now = SystemTime::now();
    let dur = now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    // simple UTC format: YYYY-MM-DDTHH:MM:SSZ
    let days = secs / 86400;
    let rem_secs = secs % 86400;
    let hours = rem_secs / 3600;
    let mins = (rem_secs % 3600) / 60;
    let s = rem_secs % 60;
    // Approximating date from epoch 1970-01-01
    let mut year = 1970;
    let mut d = days;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31
    ];
    let mut month = 1;
    for &md in &month_days {
        if d < md {
            break;
        }
        d -= md;
        month += 1;
    }
    let day = d + 1;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hours, mins, s)
}
