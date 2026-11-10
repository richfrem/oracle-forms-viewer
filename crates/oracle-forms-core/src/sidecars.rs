use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use sha2::{Digest, Sha256};
use crate::error::Result;
use crate::model::{ModuleFingerprint, SourceRef, TextEncoding};

pub const CONVERTED_DIR: &str = "converted";
pub const PREVIEW_CHARS: usize = 200;

pub struct PlsqlSidecars {
    module_cache_dir: PathBuf,
    used: HashSet<String>,
}

impl PlsqlSidecars {
    pub const TRIGGERS: &'static str = "triggers";
    pub const PROGRAM_UNITS: &'static str = "program-units";
    pub const MENU_ITEMS: &'static str = "menu-items";

    pub fn new(module_cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            module_cache_dir: module_cache_dir.into(),
            used: HashSet::new(),
        }
    }

    pub fn write(&mut self, category: &str, file_name: &str, text: &str) -> Result<SourceRef> {
        let normalized = text.replace("\r\n", "\n");
        let dir = self.module_cache_dir.join("plsql").join(category);
        fs::create_dir_all(&dir)?;

        let base = sanitize_file_name(file_name);
        let mut candidate = base.clone();
        let mut n = 2;
        while !self.used.insert(format!("{}/{}", category, candidate.to_ascii_uppercase())) {
            candidate = format!("{}~{}", base, n);
            n += 1;
        }

        let file_path = dir.join(format!("{}.sql", candidate));
        fs::write(&file_path, &normalized)?;

        let line_count = line_count_of(&normalized);
        let rel = cache_relative(&file_path, &self.module_cache_dir);

        Ok(SourceRef::new(rel, 1, line_count))
    }
}

pub fn cache_relative(file: &Path, cache_dir: &Path) -> String {
    if let Ok(rel) = file.strip_prefix(cache_dir) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        format!("{}/{}", CONVERTED_DIR, name)
    }
}

pub fn sanitize_file_name(name: &str) -> String {
    let re = regex::Regex::new(r#"[\\/:*?"<>|]"#).unwrap();
    re.replace_all(name, "_").to_string()
}

pub fn first_code_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).map(|l| l.trim()).unwrap_or("");
    if line.len() <= PREVIEW_CHARS {
        line.to_string()
    } else {
        format!("{}…", &line[..PREVIEW_CHARS])
    }
}

pub fn line_count_of(text: &str) -> usize {
    let count = text.lines().count();
    if count == 0 { 1 } else { count }
}

pub struct DecodedText {
    pub text: String,
    pub encoding: TextEncoding,
}

/// Unescapes one layer of XML character and named entities (`&#10;`, `&#x9;`, `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`).
pub fn unescape_xml_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }

    let xml_ref_re = regex::Regex::new(r"&(?:#(\d+|[xX][0-9a-fA-F]+)|(lt|gt|amp|quot|apos));").unwrap();
    let mut decoded = String::with_capacity(raw.len());
    let mut last_idx = 0;

    for cap in xml_ref_re.captures_iter(raw) {
        let m = cap.get(0).unwrap();
        decoded.push_str(&raw[last_idx..m.start()]);

        if let Some(named) = cap.get(2) {
            match named.as_str() {
                "lt" => decoded.push('<'),
                "gt" => decoded.push('>'),
                "amp" => decoded.push('&'),
                "quot" => decoded.push('"'),
                "apos" => decoded.push('\''),
                _ => decoded.push_str(m.as_str()),
            }
        } else if let Some(num_str) = cap.get(1) {
            let s = num_str.as_str();
            let code_opt = if s.starts_with('x') || s.starts_with('X') {
                u32::from_str_radix(&s[1..], 16).ok()
            } else {
                s.parse::<u32>().ok()
            };

            if let Some(code) = code_opt {
                if let Some(c) = char::from_u32(code) {
                    decoded.push(c);
                } else {
                    decoded.push_str(m.as_str());
                }
            } else {
                decoded.push_str(m.as_str());
            }
        }
        last_idx = m.end();
    }
    decoded.push_str(&raw[last_idx..]);
    decoded
}

pub fn decode_double_escaped(raw: &str) -> DecodedText {
    if raw.is_empty() || !raw.contains('&') {
        return DecodedText {
            text: raw.to_string(),
            encoding: TextEncoding::Original,
        };
    }

    let had_newline = raw.contains('\n') || raw.contains('\r');
    let pass1 = unescape_xml_entities(raw);
    let pass2 = if pass1.contains('&') {
        unescape_xml_entities(&pass1)
    } else {
        pass1
    };

    let recovered_newlines = !had_newline && (pass2.contains('\n') || pass2.contains('\r'));
    DecodedText {
        text: pass2,
        encoding: if recovered_newlines {
            TextEncoding::Recovered
        } else {
            TextEncoding::Original
        },
    }
}

pub fn fingerprint_file(path: &Path) -> Result<ModuleFingerprint> {
    let meta = fs::metadata(path)?;
    let size_bytes = meta.len();
    let last_modified_millis = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256 = format!("{:x}", hasher.finalize());

    Ok(ModuleFingerprint {
        size_bytes,
        last_modified_millis,
        sha256,
    })
}
