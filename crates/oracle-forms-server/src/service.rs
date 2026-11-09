use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use oracle_forms_binary::BinaryFmbParser;
use oracle_forms_core::error::{FormsError, Result};
use oracle_forms_core::model::*;
use oracle_forms_xml::{FormsXmlParser, PldParser};

#[derive(Default)]
struct ServiceState {
    modules: HashMap<String, ModuleIndex>,
    converted_files: HashMap<String, PathBuf>,
    cache_dirs: HashMap<String, PathBuf>,
}

#[derive(Clone)]
pub struct FormsService {
    base_cache_dir: PathBuf,
    state: Arc<RwLock<ServiceState>>,
}

impl FormsService {
    pub fn new(base_cache_dir: impl Into<PathBuf>) -> Self {
        let dir = base_cache_dir.into();
        let _ = fs::create_dir_all(&dir);
        Self {
            base_cache_dir: dir,
            state: Arc::new(RwLock::new(ServiceState::default())),
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

    pub fn load_sample(&self, sample_name: &str) -> Result<ModuleIndex> {
        let candidate_paths = [
            Self::samples_dir().join(sample_name),
            PathBuf::from(sample_name),
        ];

        for path in &candidate_paths {
            if path.is_file() {
                return self.load_module(path);
            }
        }

        Err(FormsError::NotFound(format!("Sample form '{}' not found in search paths", sample_name)))
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

        let mut active_config = config.clone();
        let driver: Box<dyn oracle_forms_db::DatabaseDriver> = match oracle_forms_db::create_driver(&active_config) {
            Ok(d) => match d.test_connection().await {
                Ok(_) => d,
                Err(e) if active_config.backend == oracle_forms_db::DbBackend::Oracle => {
                    tracing::warn!("Oracle connection failed: {}. Falling back to mock driver.", e);
                    active_config.backend = oracle_forms_db::DbBackend::Mock;
                    oracle_forms_db::create_driver(&active_config)
                        .map_err(|e| FormsError::Internal(e.to_string()))?
                }
                Err(e) => return Err(FormsError::Internal(e.to_string())),
            },
            Err(e) => return Err(FormsError::Internal(e.to_string())),
        };

        driver
            .query_block(block, limit)
            .await
            .map_err(|e| FormsError::Internal(e.to_string()))
    }

    pub fn read_source(&self, module_name: &str, relative_path: &str) -> Option<String> {
        let clean_path = relative_path.trim_start_matches(|c| c == '/' || c == '\\');
        let key = module_name.to_ascii_uppercase();
        let st = self.state.read().unwrap();

        if let Some(cache_dir) = st.cache_dirs.get(&key) {
            let sidecar_file = cache_dir.join(clean_path);
            if sidecar_file.is_file() {
                if let Ok(text) = fs::read_to_string(&sidecar_file) {
                    return Some(text);
                }
            }
        }

        if let Some(direct_converted) = st.converted_files.get(&key) {
            if let Some(parent) = direct_converted.parent() {
                let converted_relative = parent.join(clean_path);
                if converted_relative.is_file() {
                    if let Ok(text) = fs::read_to_string(&converted_relative) {
                        return Some(text);
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
