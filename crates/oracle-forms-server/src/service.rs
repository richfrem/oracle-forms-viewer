use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use oracle_forms_binary::BinaryFmbParser;
use oracle_forms_core::error::{FormsError, Result};
use oracle_forms_core::model::*;
use oracle_forms_xml::{FormsXmlParser, PldParser};
use crate::security::{is_plain_file_name, safe_join};

#[derive(Default)]
struct ServiceState {
    modules: HashMap<String, ModuleIndex>,
    converted_files: HashMap<String, PathBuf>,
    cache_dirs: HashMap<String, PathBuf>,
    /// Edits made in the layout editor, applied to the original XML on export.
    item_edits: HashMap<String, Vec<oracle_forms_xml::ItemPatch>>,
}

/// How an export was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportMode {
    /// The original Forms2XML file with only the edited attributes changed (lossless).
    PatchedOriginal,
    /// Regenerated from the parsed model: properties that are not modelled are omitted.
    Summary,
}

#[derive(Clone)]
pub struct FormsService {
    base_cache_dir: PathBuf,
    state: Arc<RwLock<ServiceState>>,
    /// When true, requests that omit DB credentials may use `OFW_DB_USER`/`OFW_DB_PASSWORD`.
    /// Off by default: a request must never be able to borrow the operator's credentials
    /// unless the operator opted in when starting the server.
    allow_env_credentials: bool,
    /// Live form sessions (triggers and PL/SQL running on the server).
    pub sessions: crate::runtime_sessions::SessionManager,
}

impl FormsService {
    pub fn new(base_cache_dir: impl Into<PathBuf>) -> Self {
        let dir = base_cache_dir.into();
        let _ = fs::create_dir_all(&dir);
        Self {
            base_cache_dir: dir,
            state: Arc::new(RwLock::new(ServiceState::default())),
            allow_env_credentials: false,
            sessions: Default::default(),
        }
    }

    /// Opt in to filling missing request credentials from the server's environment.
    pub fn with_env_credentials(mut self, allow: bool) -> Self {
        self.allow_env_credentials = allow;
        self
    }

    /// Applies the server's credential policy to a request-built config.
    pub fn apply_credential_policy(&self, config: oracle_forms_db::DbConfig) -> oracle_forms_db::DbConfig {
        if self.allow_env_credentials {
            config.with_env_credentials()
        } else {
            config
        }
    }

    pub fn samples_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("OFW_SAMPLES_DIR") {
            return PathBuf::from(dir);
        }
        if Path::new("samples").is_dir() {
            return PathBuf::from("samples");
        }
        if Path::new("../../samples").is_dir() {
            return PathBuf::from("../../samples");
        }
        if Path::new("../samples").is_dir() {
            return PathBuf::from("../samples");
        }
        PathBuf::from("samples")
    }

    pub fn get_sample_list(&self) -> Vec<String> {
        let mut list = HashSet::new();
        let samples_dir = Self::samples_dir();

        if let Ok(entries) = fs::read_dir(&samples_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let ext_lower = ext.to_ascii_lowercase();
                        if matches!(ext_lower.as_str(), "xml" | "fmb" | "pld" | "mmb" | "olb") {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                list.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }

        let mut sorted: Vec<String> = list.into_iter().collect();
        sorted.sort();
        sorted
    }

    /// Loads a module from the samples directory. Only plain file names are accepted, so a
    /// request cannot use this to open arbitrary paths.
    pub fn load_sample(&self, sample_name: &str) -> Result<ModuleIndex> {
        if !is_plain_file_name(sample_name) {
            return Err(FormsError::NotFound(format!("Invalid sample name '{}'", sample_name)));
        }
        match safe_join(&Self::samples_dir(), sample_name) {
            Some(path) if path.is_file() => self.load_module(&path),
            _ => Err(FormsError::NotFound(format!("Sample form '{}' not found in samples directory", sample_name))),
        }
    }

    pub fn load_module(&self, file_path: &Path) -> Result<ModuleIndex> {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| FormsError::InvalidFormat("Invalid file name".to_string()))?;

        let key = ModuleKey::from_file_name(file_name)
            .unwrap_or_else(|| ModuleKey::new("UNKNOWN", ModuleType::Form));

        let target_cache_dir = self.base_cache_dir.join(file_name.replace('.', "_"));
        fs::create_dir_all(&target_cache_dir)?;

        let lower_name = file_name.to_ascii_lowercase();
        let (mut index, effective_path) = if lower_name.ends_with(".pld") {
            (PldParser::parse(&key, file_path, &target_cache_dir)?, file_path.to_path_buf())
        } else if lower_name.ends_with(".xml") {
            (FormsXmlParser::parse(&key, file_path, &target_cache_dir)?, file_path.to_path_buf())
        } else {
            // Binary form (.fmb, .mmb, .olb)
            // Check if sibling XML exists in known candidate paths (matches Kotlin ModuleConverter)
            if let Some(sibling_xml) = Self::find_sibling_xml(file_path, &key) {
                tracing::info!("Resolved sibling XML for {}: {:?}", file_name, sibling_xml);
                (FormsXmlParser::parse(&key, &sibling_xml, &target_cache_dir)?, sibling_xml)
            } else {
                // Direct Binary parsing
                (BinaryFmbParser::parse(&key, file_path, &target_cache_dir)?, file_path.to_path_buf())
            }
        };

        // If form has an associated MenuModule and no menus yet, resolve companion menu XML
        if index.menus.is_empty() {
            if let Some(ref mm) = index.menu_module {
                if let Some(menu_xml) = Self::find_menu_xml(file_path, mm) {
                    tracing::info!("Resolved menu module XML for {}: {:?}", mm, menu_xml);
                    let menu_key = ModuleKey::new(mm, ModuleType::Menu);
                    if let Ok(menu_index) = FormsXmlParser::parse(&menu_key, &menu_xml, &target_cache_dir) {
                        index.menus = menu_index.menus;
                        for pu in menu_index.program_units {
                            if !index.program_units.iter().any(|existing| existing.name.eq_ignore_ascii_case(&pu.name)) {
                                index.program_units.push(pu);
                            }
                        }
                    }
                }
            }
        }

        let mut st = self.state.write().unwrap();
        let key_name = key.name.to_ascii_uppercase();
        st.modules.insert(key_name.clone(), index.clone());
        st.converted_files.insert(key_name.clone(), effective_path);
        st.cache_dirs.insert(key_name, target_cache_dir);

        Ok(index)
    }

    fn find_sibling_xml(file_path: &Path, key: &ModuleKey) -> Option<PathBuf> {
        let base_name = &key.name;
        let ext_type = key.module_type.extension();
        let candidates = [
            format!("{}_{}.xml", base_name.to_ascii_lowercase(), ext_type),
            format!("{}_{}.xml", base_name.to_ascii_uppercase(), ext_type.to_ascii_uppercase()),
            format!("{}_{}.XML", base_name.to_ascii_uppercase(), ext_type.to_ascii_uppercase()),
            format!("{}.xml", base_name.to_ascii_lowercase()),
            format!("{}.xml", base_name.to_ascii_uppercase()),
            format!("{}.XML", base_name.to_ascii_uppercase()),
        ];

        let mut search_dirs = Vec::new();
        if let Some(parent) = file_path.parent() {
            if parent.as_os_str().is_empty() {
                search_dirs.push(PathBuf::from("."));
            } else {
                search_dirs.push(parent.to_path_buf());
            }
        }
        search_dirs.push(Self::samples_dir());

        for dir in search_dirs {
            if dir.is_dir() {
                for cand in &candidates {
                    let p = dir.join(cand);
                    if p.is_file() {
                        return Some(p);
                    }
                }
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        for cand in &candidates {
                            if fname.eq_ignore_ascii_case(cand) {
                                let p = entry.path();
                                if p.is_file() {
                                    return Some(p);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn find_menu_xml(file_path: &Path, menu_module_name: &str) -> Option<PathBuf> {
        let base_name = menu_module_name.trim();
        let candidates = [
            format!("{}_mmb.xml", base_name.to_ascii_lowercase()),
            format!("{}_mmb.xml", base_name.to_ascii_uppercase()),
            format!("{}.mmb.xml", base_name.to_ascii_lowercase()),
            format!("{}.xml", base_name.to_ascii_lowercase()),
            format!("{}.xml", base_name.to_ascii_uppercase()),
        ];

        let mut search_dirs = Vec::new();
        if let Some(parent) = file_path.parent() {
            if parent.as_os_str().is_empty() {
                search_dirs.push(PathBuf::from("."));
            } else {
                search_dirs.push(parent.to_path_buf());
            }
        }
        search_dirs.push(Self::samples_dir());

        for dir in search_dirs {
            if dir.is_dir() {
                for cand in &candidates {
                    let p = dir.join(cand);
                    if p.is_file() {
                        return Some(p);
                    }
                }
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        for cand in &candidates {
                            if fname.eq_ignore_ascii_case(cand) {
                                let p = entry.path();
                                if p.is_file() {
                                    return Some(p);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn get_module(&self, module_name: &str) -> Option<ModuleIndex> {
        let st = self.state.read().unwrap();
        st.modules.get(&module_name.to_ascii_uppercase()).cloned()
    }

    pub async fn query_block(
        &self,
        module_name: &str,
        block_name: Option<&str>,
        config: &oracle_forms_db::DbConfig,
        limit: Option<usize>,
    ) -> Result<oracle_forms_db::QueryResult> {
        let module = match self.get_module(module_name) {
            Some(m) => m,
            None => {
                self.load_sample(module_name)
                    .or_else(|_| self.load_sample(&format!("{}.xml", module_name)))
                    .or_else(|_| self.load_sample(&format!("{}_fmb.xml", module_name)))
                    .or_else(|_| self.load_sample(&format!("{}.fmb", module_name)))
                    .map_err(|_| FormsError::NotFound(format!("Module '{}' not loaded in session or samples", module_name)))?
            }
        };

        let block = match block_name {
            Some(b_name) => module
                .blocks
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(b_name))
                .ok_or_else(|| FormsError::NotFound(format!("Block '{}' not found in module", b_name)))?,
            None => module
                .blocks
                .iter()
                .find(|b| b.query_data_source_name.is_some() || !b.items.is_empty())
                .ok_or_else(|| FormsError::NotFound("No queryable blocks found in module".to_string()))?,
        };

        // No fallback to the mock driver: if the requested database is unreachable the
        // caller gets the error, never fake rows that look real.
        let active_config = self.apply_credential_policy(config.clone());
        let driver = oracle_forms_db::create_driver(&active_config)
            .map_err(|e| FormsError::Database(e.to_string()))?;
        driver
            .test_connection()
            .await
            .map_err(|e| FormsError::Database(e.to_string()))?;

        driver
            .query_block(block, limit)
            .await
            .map_err(|e| FormsError::Internal(e.to_string()))
    }

    /// Reads a sidecar source file for a loaded module. `relative_path` comes from the
    /// request, so it is confined to the module's cache directory (or the directory of the
    /// module file itself) via [`safe_join`].
    pub fn read_source(&self, module_name: &str, relative_path: &str) -> Option<String> {
        // Absolute paths are rejected by `safe_join`, never reinterpreted as relative.
        let clean_path = relative_path;
        let key = module_name.to_ascii_uppercase();
        let st = self.state.read().unwrap();

        if let Some(cache_dir) = st.cache_dirs.get(&key) {
            if let Some(sidecar_file) = safe_join(cache_dir, clean_path) {
                if sidecar_file.is_file() {
                    if let Ok(text) = fs::read_to_string(&sidecar_file) {
                        return Some(text);
                    }
                }
            }
        }

        if let Some(direct_converted) = st.converted_files.get(&key) {
            if let Some(parent) = direct_converted.parent() {
                let parent = if parent.as_os_str().is_empty() { Path::new(".") } else { parent };
                if let Some(converted_relative) = safe_join(parent, clean_path) {
                    if converted_relative.is_file() {
                        if let Ok(text) = fs::read_to_string(&converted_relative) {
                            return Some(text);
                        }
                    }
                }
            }
        }

        None
    }

    pub fn read_xml_slice(&self, module_name: &str, start_line: usize, end_line: usize) -> Option<String> {
        let key = module_name.to_ascii_uppercase();
        let st = self.state.read().unwrap();
        let file = st.converted_files.get(&key)?;

        if !file.is_file() {
            return None;
        }

        let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.to_ascii_lowercase().ends_with(".fmb") {
            let file_size = fs::metadata(file).map(|m| m.len()).unwrap_or(0);
            return Some(format!(
                "<!-- This module was parsed in Direct Binary .FMB mode.\n     Source file: {} ({} bytes)\n     All PL/SQL triggers, program units, and form objects were extracted directly from the binary.\n     To inspect raw XML elements, convert this module with Oracle's frmf2xml utility. -->",
                file_name, file_size
            ));
        }

        let content = fs::read_to_string(file).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return Some(String::new());
        }

        let s = start_line.saturating_sub(1).min(lines.len().saturating_sub(1));
        let e = end_line.saturating_sub(1).min(lines.len().saturating_sub(1)).max(s);

        Some(lines[s..=e].join("\n"))
    }

    pub fn resolve_effective_item(&self, module_name: &str, item: &ItemInfo) -> EffectiveItemProperties {
        let index = match self.get_module(module_name) {
            Some(idx) => idx,
            None => {
                return EffectiveItemProperties {
                    width: item.width,
                    height: item.height,
                    x_position: item.x_position,
                    y_position: item.y_position,
                    prompt: item.prompt.clone(),
                    item_type: item.item_type.clone(),
                    dml: item.dml.clone(),
                    resolved_through: Vec::new(),
                }
            }
        };

        let mut eff_width = item.width;
        let mut eff_height = item.height;
        let eff_prompt = item.prompt.clone();
        let eff_type = item.item_type.clone();
        let mut eff_dml = item.dml.clone().unwrap_or_default();
        let mut through = Vec::new();

        if let Some(ref class_name) = item.property_class {
            let mut visited = HashSet::new();
            let chain = resolve_class_chain(&index, class_name, &mut visited, 0);
            through.extend(chain.through);
            if eff_width.is_none() {
                eff_width = chain.geometry.width;
            }
            if eff_height.is_none() {
                eff_height = chain.geometry.height;
            }
            eff_dml = eff_dml.over(&chain.dml);
        }

        EffectiveItemProperties {
            width: eff_width,
            height: eff_height,
            x_position: item.x_position,
            y_position: item.y_position,
            prompt: eff_prompt,
            item_type: eff_type,
            dml: Some(eff_dml),
            resolved_through: through,
        }
    }

    #[allow(dead_code)]
    pub fn save_module(&self, module: ModuleIndex) {
        let mut st = self.state.write().unwrap();
        st.modules.insert(module.key.name.clone(), module);
    }

    #[allow(clippy::too_many_arguments, dead_code)]
    pub fn update_item(
        &self,
        module_name: &str,
        block_name: &str,
        item_name: &str,
        x: Option<i32>,
        y: Option<i32>,
        width: Option<i32>,
        height: Option<i32>,
        prompt: Option<String>,
        label: Option<String>,
    ) -> Result<ModuleIndex> {
        self.update_item_ext(
            module_name,
            crate::routes::UpdateItemRequest {
                block_name: block_name.to_string(),
                item_name: item_name.to_string(),
                x,
                y,
                width,
                height,
                prompt,
                label,
                ..Default::default()
            },
        )
    }

    pub fn update_item_ext(
        &self,
        module_name: &str,
        req: crate::routes::UpdateItemRequest,
    ) -> Result<ModuleIndex> {
        let mod_upper = module_name.to_ascii_uppercase();
        if !self.state.read().unwrap().modules.contains_key(&mod_upper) {
            let loaded = self.get_or_load_module(module_name)?;
            self.save_module(loaded);
        }

        let mut st = self.state.write().unwrap();
        let module = st
            .modules
            .get_mut(&mod_upper)
            .ok_or_else(|| FormsError::NotFound(format!("Module '{}' not found in service state", module_name)))?;

        let block = module
            .blocks
            .iter_mut()
            .find(|b| b.name.eq_ignore_ascii_case(&req.block_name))
            .ok_or_else(|| FormsError::NotFound(format!("Block '{}' not found in module '{}'", req.block_name, module_name)))?;

        let item = block
            .items
            .iter_mut()
            .find(|i| i.name.eq_ignore_ascii_case(&req.item_name))
            .ok_or_else(|| FormsError::NotFound(format!("Item '{}' not found in block '{}'", req.item_name, req.block_name)))?;

        if let Some(x_val) = req.x { item.x_position = x_val; }
        if let Some(y_val) = req.y { item.y_position = y_val; }
        if let Some(w_val) = req.width { item.width = Some(w_val); }
        if let Some(h_val) = req.height { item.height = Some(h_val); }
        if let Some(ref v) = req.prompt { item.prompt = Some(v.clone()); }
        if let Some(ref v) = req.label { item.label = Some(v.clone()); }
        if let Some(ref v) = req.item_type { item.item_type = Some(v.clone()); }
        if let Some(ref v) = req.data_type { item.data_type = Some(v.clone()); }
        if let Some(ref v) = req.column_name { item.column_name = Some(v.clone()); }
        if let Some(ref v) = req.back_color {
            item.back_color = if v.trim().is_empty() { None } else { Some(v.clone()) };
        }
        if let Some(ref v) = req.fore_color {
            item.fore_color = if v.trim().is_empty() { None } else { Some(v.clone()) };
        }
        if let Some(v) = req.enabled {
            item.enabled = v;
            item.dml.get_or_insert_with(Default::default).enabled = Some(v);
        }
        if let Some(v) = req.visible { item.visible = Some(v); }
        if let Some(v) = req.required {
            item.required = Some(v);
            item.dml.get_or_insert_with(Default::default).required = Some(v);
        }
        if let Some(ref v) = req.prompt_attachment_edge { item.prompt_attachment_edge = v.clone(); }
        if let Some(v) = req.prompt_attachment_offset { item.prompt_attachment_offset = v; }
        if let Some(ref v) = req.initial_value {
            item.dml.get_or_insert_with(Default::default).initial_value = Some(v.clone());
        }
        if let Some(v) = req.maximum_length {
            item.dml.get_or_insert_with(Default::default).maximum_length = Some(v);
        }
        let updated = module.clone();

        // Record the edit as attribute changes for a lossless export.
        let mut attrs: Vec<(String, String)> = Vec::new();
        if let Some(v) = req.x { attrs.push(("XPosition".into(), v.to_string())); }
        if let Some(v) = req.y { attrs.push(("YPosition".into(), v.to_string())); }
        if let Some(v) = req.width { attrs.push(("Width".into(), v.to_string())); }
        if let Some(v) = req.height { attrs.push(("Height".into(), v.to_string())); }
        if let Some(v) = req.prompt { attrs.push(("Prompt".into(), v)); }
        if let Some(v) = req.label { attrs.push(("Label".into(), v)); }
        if let Some(v) = req.item_type { attrs.push(("ItemType".into(), v)); }
        if let Some(v) = req.data_type { attrs.push(("DataType".into(), v)); }
        if let Some(v) = req.column_name { attrs.push(("ColumnName".into(), v)); }
        if let Some(v) = req.back_color { attrs.push(("BackColor".into(), v)); }
        if let Some(v) = req.fore_color { attrs.push(("ForegroundColor".into(), v)); }
        if let Some(v) = req.enabled { attrs.push(("Enabled".into(), if v { "true" } else { "false" }.into())); }
        if let Some(v) = req.visible { attrs.push(("Visible".into(), if v { "true" } else { "false" }.into())); }
        if let Some(v) = req.required { attrs.push(("Required".into(), if v { "true" } else { "false" }.into())); }
        if let Some(v) = req.prompt_attachment_edge { attrs.push(("PromptAttachmentEdge".into(), v)); }
        if let Some(v) = req.prompt_attachment_offset { attrs.push(("PromptAttachmentOffset".into(), v.to_string())); }
        if let Some(v) = req.initial_value { attrs.push(("InitializeValue".into(), v)); }
        if let Some(v) = req.maximum_length { attrs.push(("MaximumLength".into(), v.to_string())); }
        let edits = st.item_edits.entry(mod_upper).or_default();
        match edits.iter_mut().find(|p| p.block.eq_ignore_ascii_case(&req.block_name) && p.item.eq_ignore_ascii_case(&req.item_name)) {
            Some(existing) => {
                for (k, v) in attrs {
                    match existing.attributes.iter_mut().find(|(ek, _)| *ek == k) {
                        Some(slot) => slot.1 = v,
                        None => existing.attributes.push((k, v)),
                    }
                }
            }
            None => edits.push(oracle_forms_xml::ItemPatch { block: req.block_name, item: req.item_name, attributes: attrs }),
        }

        Ok(updated)
    }

    /// Finds and parses an attached library (`.pld`) for a module: looks next to the
    /// module file, then in the samples directory. Only plain file names are tried.
    pub fn resolve_library(&self, module_name: &str, library_name: &str) -> Option<ModuleIndex> {
        let base = std::path::Path::new(library_name.trim())
            .file_stem()
            .and_then(|s| s.to_str())?
            .to_string();
        if !is_plain_file_name(&base) {
            return None;
        }
        let mut dirs = Vec::new();
        {
            let st = self.state.read().unwrap();
            if let Some(parent) = st.converted_files.get(&module_name.to_ascii_uppercase()).and_then(|f| f.parent()) {
                dirs.push(if parent.as_os_str().is_empty() { PathBuf::from(".") } else { parent.to_path_buf() });
            }
        }
        dirs.push(Self::samples_dir());
        let candidates = [format!("{}.pld", base.to_ascii_lowercase()), format!("{}.pld", base.to_ascii_uppercase()), format!("{}.pld", base)];
        for dir in dirs {
            for cand in &candidates {
                if let Some(path) = safe_join(&dir, cand) {
                    if path.is_file() {
                        let key = ModuleKey::new(base.to_ascii_uppercase(), ModuleType::Library);
                        let cache = self.base_cache_dir.join(format!("lib_{}", base.to_ascii_lowercase()));
                        let _ = fs::create_dir_all(&cache);
                        if let Ok(idx) = PldParser::parse(&key, &path, &cache) {
                            return Some(idx);
                        }
                    }
                }
            }
        }
        None
    }

    pub fn get_or_load_module(&self, name: &str) -> Result<ModuleIndex> {
        if let Some(m) = self.get_module(name) {
            return Ok(m);
        }
        self.load_sample(name)
            .or_else(|_| self.load_sample(&format!("{}.xml", name)))
            .or_else(|_| self.load_sample(&format!("{}_fmb.xml", name)))
    }

    /// Exports a module as Forms2XML. If the module was opened from an XML file, the
    /// original file is returned with only the edited attributes changed (lossless).
    /// Otherwise (binary .fmb) a summary is regenerated from the parsed model.
    pub fn export_xml(&self, module_name: &str) -> Result<(String, ExportMode)> {
        let module = self.get_or_load_module(module_name)?;
        let key = module.key.name.to_ascii_uppercase();
        let (source, edits) = {
            let st = self.state.read().unwrap();
            (
                st.converted_files.get(&key).cloned(),
                st.item_edits.get(&key).cloned().unwrap_or_default(),
            )
        };
        if let Some(path) = source.filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("xml")).unwrap_or(false)) {
            if let Ok(original) = fs::read_to_string(&path) {
                let patched = oracle_forms_xml::patch_items(&original, &edits)
                    .map_err(|e| FormsError::InvalidFormat(e.to_string()))?;
                return Ok((patched, ExportMode::PatchedOriginal));
            }
        }
        Ok((Self::summary_xml(&module), ExportMode::Summary))
    }

    /// Regenerated (lossy) XML, clearly labelled as such.
    pub fn summary_xml(module: &ModuleIndex) -> String {
        let xml = oracle_forms_xml::FormsXmlWriter::write_string(module);
        let note = "<!-- SUMMARY EXPORT: regenerated from the Workbench's parsed model. Properties the \
Workbench does not model (fonts, colours, LOV and record-group details, most item \
properties) are omitted. Do not compile this over the original form; convert the .fmb with \
frmf2xml for a complete export. -->\n";
        match xml.find("?>") {
            Some(i) => format!("{}\n{}{}", &xml[..i + 2], note, xml[i + 2..].trim_start()),
            None => format!("{}{}", note, xml),
        }
    }

    pub fn diff_modules(&self, base_name: &str, modified_name: &str) -> Result<oracle_forms_core::diff::ModuleDiff> {
        let base = self.get_or_load_module(base_name)?;
        let modified = self.get_or_load_module(modified_name)?;
        Ok(oracle_forms_core::diff::FormDiffer::diff(&base, &modified))
    }

    pub fn get_report_samples(&self) -> Vec<ReportSampleItem> {
        let mut list = Vec::new();
        let samples_dir = Self::samples_dir();
        if let Ok(entries) = fs::read_dir(&samples_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if file_name.ends_with("_report.xml") || file_name.ends_with("_rdf.xml") {
                        if let Ok(report) = oracle_forms_xml::ReportsXmlParser::parse_file(&path) {
                            list.push(ReportSampleItem {
                                name: report.name.clone(),
                                file_name: file_name.to_string(),
                                title: report.title.unwrap_or(report.name),
                            });
                        }
                    }
                }
            }
        }
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Loads a report definition from the samples directory by plain name.
    pub fn load_report(&self, name_or_file: &str) -> Result<oracle_forms_core::report::ReportDefinition> {
        if !is_plain_file_name(name_or_file) {
            return Err(FormsError::NotFound(format!("Invalid report name '{}'", name_or_file)));
        }
        let samples_dir = Self::samples_dir();
        let candidates = [
            name_or_file.to_string(),
            format!("{}.xml", name_or_file),
            format!("{}_report.xml", name_or_file.to_ascii_lowercase()),
            format!("{}_report.xml", name_or_file.to_ascii_uppercase()),
        ];

        for candidate in &candidates {
            if let Some(path) = safe_join(&samples_dir, candidate) {
                if path.is_file() {
                    return oracle_forms_xml::ReportsXmlParser::parse_file(&path);
                }
            }
        }

        Err(FormsError::NotFound(format!("Report definition '{}' not found", name_or_file)))
    }

    pub async fn run_report(
        &self,
        name_or_file: &str,
        backend_str: &str,
        params: &HashMap<String, String>,
    ) -> Result<(oracle_forms_core::report::ReportDefinition, oracle_forms_runtime::ReportExecutionResult, String)> {
        let report = self.load_report(name_or_file)?;

        let backend = oracle_forms_db::DbBackend::parse(backend_str)
            .map_err(|e| FormsError::Database(e.to_string()))?;

        let config = self.apply_credential_policy(oracle_forms_db::DbConfig {
            backend,
            ..Default::default()
        });

        // No silent fallback to mock data when the requested database is unreachable.
        let driver = oracle_forms_db::create_driver(&config)
            .map_err(|e| FormsError::Database(e.to_string()))?;
        driver
            .test_connection()
            .await
            .map_err(|e| FormsError::Database(e.to_string()))?;

        let result = oracle_forms_runtime::ReportDataEvaluator::evaluate(&report, &*driver, params).await?;
        let html = oracle_forms_runtime::ReportRenderer::render_html(&report, &result);

        Ok((report, result, html))
    }

    /// Attaches or switches a custom MenuModule (.mmb / _mmb.xml) on an open form module.
    pub fn attach_menu(&self, module_name: &str, menu_name_or_file: &str) -> Result<ModuleIndex> {
        let mut index = self.get_or_load_module(module_name)?;
        let key_upper = index.key.name.to_ascii_uppercase();
        let target_cache_dir = {
            let st = self.state.read().unwrap();
            st.cache_dirs
                .get(&key_upper)
                .cloned()
                .unwrap_or_else(|| self.base_cache_dir.join(format!("{}_cache", key_upper)))
        };
        let _ = fs::create_dir_all(&target_cache_dir);

        if menu_name_or_file.trim().is_empty() || menu_name_or_file.eq_ignore_ascii_case("DEFAULT") {
            index.menu_module = None;
            index.menus.clear();
        } else {
            let samples_dir = Self::samples_dir();
            let menu_xml = Self::find_menu_xml(&samples_dir.join("dummy.fmb"), menu_name_or_file)
                .or_else(|| safe_join(&samples_dir, menu_name_or_file).filter(|p| p.is_file()));

            let Some(menu_path) = menu_xml else {
                return Err(FormsError::NotFound(format!("Menu module '{}' not found", menu_name_or_file)));
            };

            let menu_key = ModuleKey::from_file_name(
                menu_path.file_name().and_then(|s| s.to_str()).unwrap_or(menu_name_or_file),
            )
            .unwrap_or_else(|| ModuleKey::new(menu_name_or_file, ModuleType::Menu));

            let menu_index = if menu_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.eq_ignore_ascii_case("xml"))
                .unwrap_or(false)
            {
                FormsXmlParser::parse(&menu_key, &menu_path, &target_cache_dir)?
            } else if let Some(sib) = Self::find_sibling_xml(&menu_path, &menu_key) {
                FormsXmlParser::parse(&menu_key, &sib, &target_cache_dir)?
            } else {
                BinaryFmbParser::parse(&menu_key, &menu_path, &target_cache_dir)?
            };

            index.menu_module = Some(menu_index.key.name.clone());
            index.menus = menu_index.menus;
            for pu in menu_index.program_units {
                if !index.program_units.iter().any(|existing| existing.name.eq_ignore_ascii_case(&pu.name)) {
                    index.program_units.push(pu);
                }
            }
        }

        let mut st = self.state.write().unwrap();
        st.modules.insert(key_upper, index.clone());
        Ok(index)
    }

    /// Lists browsable Oracle Forms & Reports artifacts across `samples/` and `samples/conformance/`.
    pub fn browse_workspace_files(&self) -> Vec<BrowsedFileItem> {
        let mut items = Vec::new();
        let samples_dir = Self::samples_dir();
        let mut dirs = vec![samples_dir.clone()];
        let conf_dir = samples_dir.join("conformance");
        if conf_dir.is_dir() {
            dirs.push(conf_dir);
        }

        for dir in dirs {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    let lower = fname.to_ascii_lowercase();
                    let category = if lower.ends_with("_report.xml")
                        || lower.ends_with("_rdf.xml")
                        || lower.ends_with(".rdf")
                        || lower.ends_with(".rpt")
                        || lower.ends_with(".rex")
                    {
                        "REPORT"
                    } else if lower.ends_with(".mmb") || lower.ends_with("_mmb.xml") {
                        "MENU"
                    } else if lower.ends_with(".olb") || lower.ends_with("_olb.xml") {
                        "OBJECT_LIB"
                    } else if lower.ends_with(".pld") || lower.ends_with(".pll") {
                        "PLSQL_LIB"
                    } else if lower.ends_with(".fmb") || lower.ends_with("_fmb.xml") || lower.ends_with(".xml") {
                        "FORM"
                    } else {
                        continue;
                    };

                    let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let rel_path = if dir.ends_with("conformance") {
                        format!("samples/conformance/{}", fname)
                    } else {
                        format!("samples/{}", fname)
                    };

                    items.push(BrowsedFileItem {
                        file_name: fname.to_string(),
                        relative_path: rel_path,
                        category: category.to_string(),
                        size_bytes,
                    });
                }
            }
        }

        items.sort_by(|a, b| a.category.cmp(&b.category).then_with(|| a.file_name.cmp(&b.file_name)));
        items
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsedFileItem {
    pub file_name: String,
    pub relative_path: String,
    pub category: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReportSampleItem {
    pub name: String,
    pub file_name: String,
    pub title: String,
}

struct ClassChain {
    dml: ItemDml,
    geometry: ItemGeometry,
    through: Vec<String>,
}

fn resolve_class_chain(
    index: &ModuleIndex,
    class_name: &str,
    visited: &mut HashSet<String>,
    hops: usize,
) -> ClassChain {
    let here = format!("{}:{}", index.key.name, class_name);
    let info = index.property_class_details.iter().find(|c| c.name.eq_ignore_ascii_case(class_name));

    if info.is_none() || hops >= 10 || !visited.insert(here.to_ascii_uppercase()) {
        return ClassChain {
            dml: ItemDml::default(),
            geometry: ItemGeometry::default(),
            through: vec![here],
        };
    }

    let info = info.unwrap();
    let own_dml = info.item.clone().unwrap_or_default();
    let own_size = info.geometry.clone().unwrap_or_default();

    let parent_chain = if let Some(ref parent_class) = info.property_class {
        resolve_class_chain(index, parent_class, visited, hops + 1)
    } else {
        ClassChain {
            dml: ItemDml::default(),
            geometry: ItemGeometry::default(),
            through: Vec::new(),
        }
    };

    let mut all_through = vec![here];
    for t in parent_chain.through {
        if !all_through.contains(&t) {
            all_through.push(t);
        }
    }

    ClassChain {
        dml: own_dml.over(&parent_chain.dml),
        geometry: own_size.over(&parent_chain.geometry),
        through: all_through,
    }
}
