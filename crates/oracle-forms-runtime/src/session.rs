use crate::interpreter::{ExecutionStatus, InteractionRequest, InteractionResponse, Interpreter};
use crate::master_detail::MasterDetailLink;
use crate::program_units::ProgramRegistry;
use crate::record::{BlockBuffer, FormsRecordStatus};
use crate::system::{OperationalMode, SystemVariables};
use crate::trigger::{ExecutionHierarchy, ResolvedTrigger, TriggerResolver, TriggerResult};
use crate::value::OracleValue;
use oracle_forms_core::model::{BlockInfo, ModuleIndex};
use oracle_forms_db::{DatabaseDriver, GeneratedQuery, QueryResult, SqlGenerator};
use oracle_forms_plsql::ast::PlsqlUnit;
use oracle_forms_plsql::Parser;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Block not found: {0}")]
    BlockNotFound(String),

    #[error("Item not found: {0}")]
    ItemNotFound(String),

    #[error("Validation failed on {item}: {message}")]
    ValidationFailed { item: String, message: String },

    #[error("Navigation failed: {0}")]
    NavigationFailed(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Trigger failure: {0}")]
    TriggerFailure(String),

    #[error("Record lock or concurrency conflict: {0}")]
    RecordLockedOrChanged(String),

    #[error("Step budget exceeded")]
    StepBudgetExceeded,

    #[error("Recursion limit exceeded")]
    RecursionLimitExceeded,

    #[error("Unhandled exception: {0}")]
    UnhandledException(String),

    /// Something the form needs that this runtime does not provide (yet). Always
    /// reported; never treated as success.
    #[error("Not supported by the Workbench runtime: {0}")]
    Unsupported(String),

    #[error("Session is read-only: {0}")]
    ReadOnly(String),
}

pub type ValidationHook =
    Box<dyn Fn(&str, &str, &serde_json::Value) -> Result<(), String> + Send + Sync>;

/// Answers `SHOW_ALERT` (and future interactive requests) synchronously. The server runs
/// each session on its own thread, so a handler may block until the browser responds.
pub type InteractionHandler = Box<dyn FnMut(&InteractionRequest) -> InteractionResponse + Send>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertDefinition {
    pub name: String,
    pub message: String,
    pub button1: String,
    pub button2: Option<String>,
    pub button3: Option<String>,
    pub default_button: u8,
}

/// Runs a future to completion from synchronous runtime code.
///
/// On a multi-threaded Tokio runtime this uses `block_in_place`; on a current-thread
/// runtime (where `block_in_place` would panic) or outside Tokio it runs the future on a
/// short-lived runtime in a scoped thread.
pub(crate) fn block_on_future<F>(fut: F) -> F::Output
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    let fresh_runtime = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create temporary tokio runtime")
    };
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| handle.block_on(fut))
        }
        Ok(_) => std::thread::scope(|scope| {
            scope
                .spawn(|| fresh_runtime().block_on(fut))
                .join()
                .expect("database task panicked")
        }),
        Err(_) => fresh_runtime().block_on(fut),
    }
}

/// Maximum nesting of triggers firing triggers (e.g. GO_ITEM inside WHEN-NEW-ITEM-INSTANCE).
const MAX_TRIGGER_DEPTH: usize = 32;

/// Active in-memory Form runtime session.
pub struct FormSession {
    pub module: ModuleIndex,
    pub blocks: HashMap<String, BlockBuffer>,
    pub system: SystemVariables,
    pub master_detail_links: Vec<MasterDetailLink>,
    pub trace_log: Vec<String>,
    /// Default Execution Hierarchy for triggers that do not set the property.
    pub hierarchy: ExecutionHierarchy,
    pub validation_hook: Option<ValidationHook>,
    pub global_variables: HashMap<String, OracleValue>,
    pub messages: Vec<String>,
    pub alerts: HashMap<String, AlertDefinition>,
    pub item_properties: HashMap<String, HashMap<String, OracleValue>>,
    /// Runtime block properties set via SET_BLOCK_PROPERTY (DEFAULT_WHERE, ORDER_BY, ...).
    pub block_properties: HashMap<String, HashMap<String, OracleValue>>,
    pub database_driver: Option<Arc<dyn DatabaseDriver>>,
    /// Database user name, for USER / GET_APPLICATION_PROPERTY(USERNAME).
    pub db_username: Option<String>,
    /// When true, DML, locks, commits and stored-procedure calls are refused. Queries run.
    pub read_only: bool,
    pub interaction_handler: Option<InteractionHandler>,
    /// Form + attached-library program units.
    pub programs: Arc<ProgramRegistry>,
    pub package_vars: HashMap<String, OracleValue>,
    pub initialized_packages: HashSet<String>,
    /// FORM_SUCCESS / FORM_FAILURE state of the most recent built-in.
    pub last_builtin_failed: bool,
    pub last_error_text: Option<String>,
    pub exit_requested: bool,
    pub sequences: std::sync::Mutex<HashMap<String, u64>>,
    parsed_triggers: HashMap<String, Result<Arc<PlsqlUnit>, String>>,
    trigger_depth: usize,
    /// Navigation happened inside a trigger: WHEN-NEW-*-INSTANCE triggers are owed once
    /// the outermost trigger finishes (Forms fires them only when input focus settles).
    focus_dirty: bool,
    settling: bool,
}

/// Where input focus is: (block, buffer generation, record index, item).
type Focus = (String, u64, usize, String);

impl FormSession {
    /// Creates a new FormSession from a parsed module definition.
    pub fn new(module: ModuleIndex) -> Self {
        let mut blocks = HashMap::new();
        let mut master_detail_links = Vec::new();
        let mut alerts = HashMap::new();

        for b in &module.blocks {
            let b_upper = b.name.to_ascii_uppercase();
            blocks.insert(b_upper.clone(), BlockBuffer::new(b.name.clone()));

            for rel in &b.relations {
                if let Some(link) = MasterDetailLink::from_relation(&b.name, rel) {
                    master_detail_links.push(link);
                }
            }
        }

        for a in &module.alerts {
            alerts.insert(
                a.name.to_ascii_uppercase(),
                AlertDefinition {
                    name: a.name.clone(),
                    message: a.message.clone().unwrap_or_else(|| format!("Alert: {}", a.name)),
                    // Forms defaults: OK / Cancel when the alert defines no labels.
                    button1: a.button1_label.clone().unwrap_or_else(|| "OK".to_string()),
                    button2: if a.button1_label.is_some() || a.button2_label.is_some() {
                        a.button2_label.clone()
                    } else {
                        Some("Cancel".to_string())
                    },
                    button3: a.button3_label.clone(),
                    default_button: a.default_button.unwrap_or(1),
                },
            );
        }

        let programs = Arc::new(ProgramRegistry::from_module(&module));

        Self {
            module,
            blocks,
            system: SystemVariables::default(),
            master_detail_links,
            trace_log: Vec::new(),
            hierarchy: ExecutionHierarchy::Override,
            validation_hook: None,
            global_variables: HashMap::new(),
            messages: Vec::new(),
            alerts,
            item_properties: HashMap::new(),
            block_properties: HashMap::new(),
            database_driver: None,
            db_username: None,
            read_only: false,
            interaction_handler: None,
            programs,
            package_vars: HashMap::new(),
            initialized_packages: HashSet::new(),
            last_builtin_failed: false,
            last_error_text: None,
            exit_requested: false,
            sequences: std::sync::Mutex::new(HashMap::new()),
            parsed_triggers: HashMap::new(),
            trigger_depth: 0,
            focus_dirty: false,
            settling: false,
        }
    }

    /// Makes an attached library's program units callable from this form.
    pub fn attach_library(&mut self, library: &ModuleIndex) {
        let mut reg = (*self.programs).clone();
        reg.attach_library(library);
        self.programs = Arc::new(reg);
    }

    /// Sets custom validation hook for simulating trigger validation logic.
    pub fn set_validation_hook(&mut self, hook: ValidationHook) {
        self.validation_hook = Some(hook);
    }

    /// Sets attached database driver for embedded SQL and query execution.
    pub fn set_database_driver(&mut self, driver: Arc<dyn DatabaseDriver>) {
        self.database_driver = Some(driver);
    }

    pub fn set_interaction_handler(&mut self, handler: InteractionHandler) {
        self.interaction_handler = Some(handler);
    }

    /// Returns the currently active block name, or default if unset.
    pub fn active_block(&self) -> Option<&str> {
        self.system.cursor_block.as_deref().or_else(|| {
            self.module.blocks.first().map(|b| b.name.as_str())
        })
    }

    fn block_info(&self, name: &str) -> Option<&BlockInfo> {
        self.module.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// A base-table block (has a data source and is not marked as a control block).
    /// Records in control blocks are never posted to the database.
    fn is_database_block(&self, name: &str) -> bool {
        self.block_info(name)
            .map(|b| {
                b.dml.as_ref().and_then(|d| d.database_block) != Some(false)
                    && SqlGenerator::dml_table(b).is_ok()
            })
            .unwrap_or(false)
    }

    /// Gets a bind variable value (`:BLOCK.ITEM`, `:SYSTEM.*`, `:GLOBAL.*`, `:PARAMETER.*`).
    pub fn get_bind_value(&self, bind: &str) -> OracleValue {
        let clean = bind.trim().trim_start_matches(':').to_ascii_uppercase();

        if let Some(var_name) = clean.strip_prefix("SYSTEM.") {
            return self.system.get_var(var_name).map(OracleValue::varchar2).unwrap_or(OracleValue::Null);
        }

        if let Some(var_name) = clean.strip_prefix("GLOBAL.") {
            return self.global_variables.get(var_name).cloned().unwrap_or(OracleValue::Null);
        }

        if let Some(param) = clean.strip_prefix("PARAMETER.") {
            return self
                .global_variables
                .get(&format!("PARAMETER.{}", param))
                .cloned()
                .or_else(|| {
                    self.module
                        .parameters
                        .iter()
                        .find(|p| p.name.eq_ignore_ascii_case(param))
                        .and_then(|p| p.default_value.clone())
                        .map(OracleValue::varchar2)
                })
                .unwrap_or(OracleValue::Null);
        }

        if let Some(seq_part) = clean.strip_prefix("SEQUENCE.") {
            // Local stand-in used only when no Oracle database is attached (the
            // interpreter asks the database first).
            let seq_name = seq_part.split('.').next().unwrap_or(seq_part).to_string();
            let mut lock = self.sequences.lock().unwrap();
            let entry = lock.entry(seq_name).or_insert(1000);
            *entry += 1;
            return OracleValue::int(*entry as i64);
        }

        let (block_name, item_name) = match clean.split_once('.') {
            Some((b, i)) => (b.to_string(), i.to_string()),
            None => match self.active_block() {
                Some(active) => (active.to_ascii_uppercase(), clean.clone()),
                None => return OracleValue::Null,
            },
        };
        let value = self
            .blocks
            .get(&block_name)
            .and_then(|buf| buf.current_record())
            .and_then(|rec| rec.get_value(&item_name))
            .map(OracleValue::from_json)
            .unwrap_or(OracleValue::Null);
        // Items carry a Forms data type (Date, Number, Char): values read from them
        // have that type, e.g. a DATE item is bound to SQL as a date, not as text.
        match self
            .block_info(&block_name)
            .and_then(|b| b.items.iter().find(|i| i.name.eq_ignore_ascii_case(&item_name)))
            .and_then(|i| i.data_type.clone())
        {
            Some(t) => value.coerce_to(&t),
            None => value,
        }
    }

    /// Sets a bind variable value (`:BLOCK.ITEM`, `:SYSTEM.*`, `:GLOBAL.*`, `:PARAMETER.*`).
    pub fn set_bind_value(&mut self, bind: &str, val: OracleValue) {
        let clean = bind.trim().trim_start_matches(':').to_ascii_uppercase();

        if let Some(var_name) = clean.strip_prefix("SYSTEM.") {
            if let Some(s) = val.as_text() {
                self.system.set_var(var_name, &s);
            }
            return;
        }

        if let Some(var_name) = clean.strip_prefix("GLOBAL.") {
            // Forms globals are always character strings.
            let text = val.as_text().map(OracleValue::varchar2).unwrap_or(OracleValue::Null);
            self.global_variables.insert(var_name.to_string(), text);
            return;
        }

        if clean.starts_with("PARAMETER.") {
            self.global_variables.insert(clean, val);
            return;
        }

        let (block_name, item_name) = match clean.split_once('.') {
            Some((b, i)) => (b.to_string(), i.to_string()),
            None => match self.active_block() {
                Some(active) => (active.to_ascii_uppercase(), clean.clone()),
                None => return,
            },
        };
        if let Some(buf) = self.blocks.get_mut(&block_name) {
            if buf.records.is_empty() {
                buf.create_record();
            }
            if let Some(rec) = buf.current_record_mut() {
                rec.set_value(&item_name, val.to_json());
            }
        }
    }

    /// Erases a global session variable (`ERASE('GLOBAL.X')`).
    pub fn erase_global(&mut self, name: &str) {
        let clean = name.trim().trim_start_matches(':').to_ascii_uppercase();
        let clean = clean.trim_start_matches("GLOBAL.").to_string();
        self.global_variables.remove(&clean);
    }

    /// Gets an item property.
    pub fn get_item_property(&self, item: &str, prop: &str) -> OracleValue {
        self.item_properties
            .get(&item.to_ascii_uppercase())
            .and_then(|p| p.get(&prop.to_ascii_uppercase()))
            .cloned()
            .unwrap_or(OracleValue::Null)
    }

    /// Sets an item property.
    pub fn set_item_property(&mut self, item: &str, prop: &str, val: OracleValue) {
        self.item_properties
            .entry(item.to_ascii_uppercase())
            .or_default()
            .insert(prop.to_ascii_uppercase(), val);
    }

    pub fn set_block_property(&mut self, block: &str, prop: &str, val: OracleValue) {
        self.block_properties
            .entry(block.to_ascii_uppercase())
            .or_default()
            .insert(prop.to_ascii_uppercase(), val);
    }

    pub fn get_block_property(&self, block: &str, prop: &str) -> OracleValue {
        let b = block.to_ascii_uppercase();
        let p = prop.to_ascii_uppercase();
        if let Some(v) = self.block_properties.get(&b).and_then(|m| m.get(&p)) {
            return v.clone();
        }
        let Some(info) = self.block_info(&b) else { return OracleValue::Null };
        match p.as_str() {
            "DEFAULT_WHERE" => info.dml.as_ref().and_then(|d| d.where_clause.clone()).map(OracleValue::varchar2).unwrap_or_default(),
            "ORDER_BY" => info.dml.as_ref().and_then(|d| d.order_by_clause.clone()).map(OracleValue::varchar2).unwrap_or_default(),
            "QUERY_DATA_SOURCE_NAME" => info.query_data_source_name.clone().map(OracleValue::varchar2).unwrap_or_default(),
            "CURRENT_RECORD" => self.blocks.get(&b).map(|buf| OracleValue::int(buf.current_record_index as i64 + 1)).unwrap_or_default(),
            "STATUS" => self.blocks.get(&b).map(|buf| OracleValue::varchar2(buf.status().as_str())).unwrap_or_default(),
            _ => OracleValue::Null,
        }
    }

    /// Clears records in a block.
    pub fn clear_block(&mut self, block_name: &str) {
        if let Some(buf) = self.blocks.get_mut(&block_name.to_ascii_uppercase()) {
            buf.clear_block();
        }
        self.update_system_status();
    }

    /// Clears all blocks in the form.
    pub fn clear_form(&mut self) {
        for buf in self.blocks.values_mut() {
            buf.clear_block();
        }
        self.update_system_status();
    }

    // ------------------------------------------------------------------
    // Trigger dispatch
    // ------------------------------------------------------------------

    fn parse_trigger(&mut self, trigger: &ResolvedTrigger) -> Option<Result<Arc<PlsqlUnit>, String>> {
        let info = trigger.trigger_info.as_ref()?;
        let code = info.source_text.as_deref().unwrap_or(&info.first_line);
        if code.trim().is_empty() {
            return None;
        }
        let key = format!(
            "{}|{}|{}",
            trigger.block_name.as_deref().unwrap_or(""),
            trigger.item_name.as_deref().unwrap_or(""),
            trigger.name
        )
        .to_ascii_uppercase();
        if let Some(cached) = self.parsed_triggers.get(&key) {
            return Some(cached.clone());
        }
        let mut parser = Parser::new(code);
        let parsed = match parser.parse_unit() {
            Ok(unit) if parser.errors.is_empty() => Ok(Arc::new(unit)),
            Ok(_) => Err(parser.errors.join("; ")),
            Err(e) => Err(e.to_string()),
        };
        self.parsed_triggers.insert(key, parsed.clone());
        Some(parsed)
    }

    fn focus(&self) -> Focus {
        let block = self.system.cursor_block.clone().unwrap_or_default().to_ascii_uppercase();
        let (generation, record) = self
            .blocks
            .get(&block)
            .map(|b| (b.generation, b.current_record_index))
            .unwrap_or((0, 0));
        let item = self.system.cursor_item.clone().unwrap_or_default().to_ascii_uppercase();
        (block, generation, record, item)
    }

    /// Fires WHEN-NEW-*-INSTANCE now (outside triggers) or defers it until the outermost
    /// trigger completes, matching Forms: these triggers fire when input focus settles,
    /// for the final location only, not for each intermediate GO_BLOCK / NEXT_RECORD.
    fn fire_when_new(&mut self, events: &[(&str, Option<&str>, Option<&str>)]) {
        if self.trigger_depth > 0 {
            self.focus_dirty = true;
            return;
        }
        for (event, item, block) in events {
            self.dispatch_event(event, *item, *block);
        }
    }

    /// After a trigger moved the cursor: fire the WHEN-NEW triggers for where focus ended.
    fn settle_focus(&mut self, before: Focus) {
        self.settling = true;
        let mut before = before;
        for _ in 0..8 {
            if !self.focus_dirty {
                break;
            }
            self.focus_dirty = false;
            let now = self.focus();
            if now == before {
                break;
            }
            let block = self.system.cursor_block.clone().unwrap_or_default();
            let item = self
                .system
                .cursor_item
                .as_deref()
                .and_then(|ci| ci.split_once('.'))
                .map(|(_, i)| i.to_string());
            if now.0 != before.0 {
                self.dispatch_event("WHEN-NEW-BLOCK-INSTANCE", None, Some(&block));
            }
            if now.0 != before.0 || now.1 != before.1 || now.2 != before.2 {
                self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(&block));
            }
            self.dispatch_event("WHEN-NEW-ITEM-INSTANCE", item.as_deref(), Some(&block));
            before = now;
        }
        self.focus_dirty = false;
        self.settling = false;
    }

    /// Fires an event: resolves the trigger(s) per Execution Hierarchy and runs them.
    pub fn dispatch_event(&mut self, event_name: &str, item: Option<&str>, block: Option<&str>) -> TriggerResult {
        let outermost = self.trigger_depth == 0 && !self.settling;
        let before = if outermost { Some(self.focus()) } else { None };
        let result = self.dispatch_event_inner(event_name, item, block);
        if let Some(before) = before {
            if self.focus_dirty {
                self.settle_focus(before);
            }
        }
        result
    }

    fn dispatch_event_inner(&mut self, event_name: &str, item: Option<&str>, block: Option<&str>) -> TriggerResult {
        let candidates = TriggerResolver::resolve(event_name, item, block, &self.module.triggers, self.hierarchy);

        if self.trigger_depth >= MAX_TRIGGER_DEPTH {
            self.trace_log.push(format!("TRIGGER_DEPTH_EXCEEDED: {}", event_name));
            return TriggerResult::Failure("Trigger nesting too deep (recursive navigation?)".to_string());
        }

        for trigger in candidates {
            let trace_entry = match (&trigger.block_name, &trigger.item_name) {
                (Some(b), Some(i)) => format!("{}({}.{})", trigger.name, b, i),
                (Some(b), None) => format!("{}({})", trigger.name, b),
                _ => trigger.name.clone(),
            };
            self.trace_log.push(trace_entry);

            if let Some(parsed) = self.parse_trigger(&trigger) {
                let unit = match parsed {
                    Ok(u) => u,
                    Err(e) => {
                        // A trigger we cannot parse must not silently "succeed".
                        let msg = format!("{} could not be parsed: {}", trigger.name, e);
                        self.trace_log.push(format!("TRIGGER_PARSE_ERROR: {}", msg));
                        self.messages.push(msg.clone());
                        return TriggerResult::Failure(msg);
                    }
                };

                let saved_trigger_item = (self.system.trigger_block.clone(), self.system.trigger_item.clone());
                self.system.trigger_block = trigger.block_name.clone().or_else(|| self.system.cursor_block.clone());
                self.system.trigger_item = match (&trigger.block_name, &trigger.item_name) {
                    (Some(b), Some(i)) => Some(format!("{}.{}", b, i)),
                    _ => self.system.cursor_item.clone(),
                };

                self.trigger_depth += 1;
                let mut interpreter = Interpreter::new();
                let outcome = interpreter.execute_unit(&unit, self);
                self.trigger_depth -= 1;
                (self.system.trigger_block, self.system.trigger_item) = saved_trigger_item;

                match outcome {
                    Ok(ExecutionStatus::Success(_)) => {}
                    Ok(ExecutionStatus::TriggerFailure(msg)) => {
                        self.trace_log.push(format!("TRIGGER_FAILURE: {}", msg));
                        return TriggerResult::Failure(msg);
                    }
                    Ok(ExecutionStatus::UnhandledException(ex)) => {
                        // FRM-40735: trigger raised unhandled exception.
                        self.trace_log.push(format!("UNHANDLED_EXCEPTION: {}", ex));
                        self.messages.push(format!("FRM-40735: {} trigger raised unhandled exception {}", trigger.name, ex));
                        return TriggerResult::UnhandledException(ex);
                    }
                    Err(e) => {
                        self.trace_log.push(format!("RUNTIME_ERROR: {}", e));
                        self.messages.push(format!("{} trigger: {}", trigger.name, e));
                        return TriggerResult::UnhandledException(e.to_string());
                    }
                }
            }

            // Test hook for WHEN-VALIDATE-ITEM (lets tests simulate validation logic).
            if event_name.eq_ignore_ascii_case("WHEN-VALIDATE-ITEM") {
                if let (Some(b_name), Some(i_name)) = (block, item) {
                    if let Some(hook) = &self.validation_hook {
                        let val = self
                            .blocks
                            .get(&b_name.to_ascii_uppercase())
                            .and_then(|buf| buf.current_record())
                            .and_then(|rec| rec.get_value(i_name).cloned())
                            .unwrap_or(serde_json::Value::Null);
                        if let Err(err_msg) = hook(b_name, i_name, &val) {
                            self.trace_log.push(format!("TRIGGER_FAILURE: {}", err_msg));
                            return TriggerResult::Failure(err_msg);
                        }
                    }
                }
            }
        }

        TriggerResult::Success
    }

    /// True if a trigger for `event` exists at block or form level for `block`.
    fn has_trigger(&self, event: &str, block: &str) -> bool {
        self.module.triggers.iter().any(|t| {
            t.name.eq_ignore_ascii_case(event)
                && t.item_name.is_none()
                && match &t.block_name {
                    Some(b) => b.eq_ignore_ascii_case(block),
                    None => true,
                }
        })
    }

    /// `DO_KEY('NAME')`: fires the matching KEY- trigger if one exists, otherwise runs the
    /// default built-in.
    pub fn do_key(&mut self, builtin: &str) -> Result<(), RuntimeError> {
        let up = builtin.trim().to_ascii_uppercase();
        let key_trigger = match up.as_str() {
            "COMMIT_FORM" => "KEY-COMMIT",
            "EXECUTE_QUERY" => "KEY-EXEQRY",
            "ENTER_QUERY" => "KEY-ENTQRY",
            "NEXT_ITEM" => "KEY-NEXT-ITEM",
            "PREVIOUS_ITEM" => "KEY-PREV-ITEM",
            "NEXT_RECORD" => "KEY-NXTREC",
            "PREVIOUS_RECORD" => "KEY-PRVREC",
            "CREATE_RECORD" => "KEY-CREREC",
            "DELETE_RECORD" => "KEY-DELREC",
            "EXIT_FORM" => "KEY-EXIT",
            "CLEAR_FORM" => "KEY-CLRFRM",
            "CLEAR_BLOCK" => "KEY-CLRBLK",
            "NEXT_BLOCK" => "KEY-NXTBLK",
            "PREVIOUS_BLOCK" => "KEY-PRVBLK",
            "LIST_VALUES" => "KEY-LISTVAL",
            _ => "",
        };
        let block = self.active_block().map(str::to_string);
        let (item_block, item) = self
            .system
            .cursor_item
            .as_deref()
            .and_then(|ci| ci.split_once('.'))
            .map(|(b, i)| (Some(b.to_string()), Some(i.to_string())))
            .unwrap_or((None, None));
        let exists = !key_trigger.is_empty()
            && self.module.triggers.iter().any(|t| t.name.eq_ignore_ascii_case(key_trigger));
        if exists {
            let b = item_block.or(block);
            let res = self.dispatch_event(key_trigger, item.as_deref(), b.as_deref());
            return match res {
                TriggerResult::Success => Ok(()),
                TriggerResult::Failure(m) => Err(RuntimeError::TriggerFailure(m)),
                TriggerResult::UnhandledException(m) => Err(RuntimeError::UnhandledException(m)),
            };
        }
        self.run_default_key(&up)
    }

    fn run_default_key(&mut self, builtin: &str) -> Result<(), RuntimeError> {
        match builtin {
            "COMMIT_FORM" => self.commit(),
            "EXECUTE_QUERY" => match self.active_block().map(str::to_string) {
                Some(b) => self.execute_query_on_block(&b),
                None => Ok(()),
            },
            "ENTER_QUERY" => {
                self.system.mode = OperationalMode::EnterQuery;
                Ok(())
            }
            "NEXT_ITEM" => self.next_item(),
            "PREVIOUS_ITEM" => self.previous_item(),
            "NEXT_RECORD" => self.next_record(),
            "PREVIOUS_RECORD" => self.previous_record(),
            "CREATE_RECORD" => self.create_record(),
            "DELETE_RECORD" => self.delete_record(),
            "EXIT_FORM" => {
                self.exit_requested = true;
                Ok(())
            }
            "CLEAR_FORM" => {
                self.clear_form();
                Ok(())
            }
            "CLEAR_BLOCK" => {
                if let Some(b) = self.active_block().map(str::to_string) {
                    self.clear_block(&b);
                }
                Ok(())
            }
            other => Err(RuntimeError::Unsupported(format!("DO_KEY('{}')", other))),
        }
    }

    /// Executes canonical Oracle Forms startup cascade:
    /// PRE-FORM -> PRE-BLOCK -> PRE-RECORD -> PRE-TEXT-ITEM ->
    /// WHEN-NEW-FORM-INSTANCE -> WHEN-NEW-BLOCK-INSTANCE ->
    /// WHEN-NEW-RECORD-INSTANCE -> WHEN-NEW-ITEM-INSTANCE
    pub fn start(&mut self) -> Result<(), RuntimeError> {
        let first_block = self
            .module
            .blocks
            .first()
            .map(|b| b.name.clone())
            .ok_or_else(|| RuntimeError::BlockNotFound("form has no blocks".to_string()))?;

        let first_item = self
            .module
            .blocks
            .first()
            .and_then(|b| {
                b.items.iter().find(|i| {
                    !i.item_type
                        .as_deref()
                        .map(|t| t.eq_ignore_ascii_case("Push Button"))
                        .unwrap_or(false)
                })
            })
            .or_else(|| self.module.blocks.first().and_then(|b| b.items.first()))
            .map(|i| i.name.clone())
            .unwrap_or_default();

        self.system.cursor_block = Some(first_block.clone());
        self.system.cursor_item = Some(format!("{}.{}", first_block, first_item));
        self.system.cursor_record = 1;
        self.system.record_status = FormsRecordStatus::New;
        self.system.block_status = FormsRecordStatus::New;
        self.system.form_status = FormsRecordStatus::New;
        self.system.mode = OperationalMode::Normal;

        // PRE-FORM failure aborts startup in Forms.
        if let TriggerResult::Failure(m) | TriggerResult::UnhandledException(m) = self.dispatch_event("PRE-FORM", None, None) {
            return Err(RuntimeError::TriggerFailure(format!("PRE-FORM: {}", m)));
        }
        self.dispatch_event("PRE-BLOCK", None, Some(&first_block));
        self.dispatch_event("PRE-RECORD", None, Some(&first_block));
        self.dispatch_event("PRE-TEXT-ITEM", Some(&first_item), Some(&first_block));
        let initial = self.focus();
        self.dispatch_event("WHEN-NEW-FORM-INSTANCE", None, None);
        // If WHEN-NEW-FORM-INSTANCE navigated, the WHEN-NEW triggers already fired for the
        // location focus settled on.
        if self.focus() == initial {
            self.dispatch_event("WHEN-NEW-BLOCK-INSTANCE", None, Some(&first_block));
            self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(&first_block));
            self.dispatch_event("WHEN-NEW-ITEM-INSTANCE", Some(&first_item), Some(&first_block));
        }

        Ok(())
    }

    /// Simulates a user pressing a push button: fires WHEN-BUTTON-PRESSED for the item.
    pub fn press_button(&mut self, item_path: &str) -> TriggerResult {
        let (block, item) = match item_path.split_once('.') {
            Some((b, i)) => (b.to_string(), i.to_string()),
            None => (self.active_block().unwrap_or_default().to_string(), item_path.to_string()),
        };
        self.dispatch_event("WHEN-BUTTON-PRESSED", Some(&item), Some(&block))
    }

    /// Navigates to a target item, executing validation cascade on departure.
    pub fn navigate_to(&mut self, target_item_path: &str) -> Result<(), RuntimeError> {
        let (target_block, target_item) = if let Some((b, i)) = target_item_path.split_once('.') {
            (b.to_string(), i.to_string())
        } else {
            let active_b = self
                .system
                .cursor_block
                .clone()
                .unwrap_or_else(|| "BLOCK1".to_string());
            (active_b, target_item_path.to_string())
        };

        // FRM-40105 / FRM-40104: the target must exist.
        let Some(target_info) = self.block_info(&target_block) else {
            return Err(RuntimeError::BlockNotFound(target_block));
        };
        if !target_info.items.iter().any(|i| i.name.eq_ignore_ascii_case(&target_item)) {
            return Err(RuntimeError::ItemNotFound(format!("{}.{}", target_block, target_item)));
        }
        let target_block = target_info.name.clone();

        let curr_item_opt = self.system.cursor_item.clone();
        let curr_block_opt = self.system.cursor_block.clone();

        // 1. Validation Cascade on Source Field (if leaving item and item is dirty)
        if let Some(curr_item) = &curr_item_opt {
            let (cb, ci) = if let Some((b, i)) = curr_item.split_once('.') {
                (b, i)
            } else {
                ("BLOCK1", curr_item.as_str())
            };

            let cb_upper = cb.to_ascii_uppercase();
            let is_dirty = self
                .blocks
                .get(&cb_upper)
                .and_then(|b| b.current_record())
                .map(|r| r.is_item_dirty(ci))
                .unwrap_or(false);

            if is_dirty {
                // Fire POST-TEXT-ITEM
                self.dispatch_event("POST-TEXT-ITEM", Some(ci), Some(cb));

                // Fire WHEN-VALIDATE-ITEM
                let val_res = self.dispatch_event("WHEN-VALIDATE-ITEM", Some(ci), Some(cb));
                if !val_res.is_success() {
                    return Err(RuntimeError::ValidationFailed {
                        item: curr_item.clone(),
                        message: "Validation failed in WHEN-VALIDATE-ITEM".to_string(),
                    });
                }

                // Validation succeeded: mark item valid
                if let Some(buf) = self.blocks.get_mut(&cb_upper) {
                    if let Some(rec) = buf.current_record_mut() {
                        rec.mark_item_valid(ci);
                    }
                }
            }
        }

        // 2. Block transition triggers if moving between blocks
        let is_block_change = curr_block_opt.as_deref().map(|b| !b.eq_ignore_ascii_case(&target_block)).unwrap_or(true);
        if is_block_change {
            if let Some(cb) = &curr_block_opt {
                self.dispatch_event("POST-BLOCK", None, Some(cb));
            }
            self.dispatch_event("PRE-BLOCK", None, Some(&target_block));
        }

        // 3. Move focus, then the enter-item triggers.
        self.system.cursor_block = Some(target_block.clone());
        self.system.cursor_item = Some(format!("{}.{}", target_block, target_item));
        if let Some(buf) = self.blocks.get(&target_block.to_ascii_uppercase()) {
            self.system.cursor_record = buf.current_record_index + 1;
        }
        self.update_system_status();
        self.dispatch_event("PRE-TEXT-ITEM", Some(&target_item), Some(&target_block));
        if is_block_change {
            self.fire_when_new(&[
                ("WHEN-NEW-BLOCK-INSTANCE", None, Some(&target_block)),
                ("WHEN-NEW-RECORD-INSTANCE", None, Some(&target_block)),
                ("WHEN-NEW-ITEM-INSTANCE", Some(&target_item), Some(&target_block)),
            ]);
        } else {
            self.fire_when_new(&[("WHEN-NEW-ITEM-INSTANCE", Some(&target_item), Some(&target_block))]);
        }

        Ok(())
    }

    /// Moves cursor to next item in active block.
    pub fn next_item(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            if let Some(block_info) = self.module.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(&cb)) {
                let items: Vec<&str> = block_info.items.iter().map(|i| i.name.as_str()).collect();
                if let Some(curr_item) = self.system.cursor_item.clone() {
                    let item_short = curr_item.split('.').nth(1).unwrap_or(&curr_item);
                    if let Some(pos) = items.iter().position(|&i| i.eq_ignore_ascii_case(item_short)) {
                        let next_idx = (pos + 1) % items.len();
                        return self.navigate_to(&format!("{}.{}", cb, items[next_idx]));
                    }
                } else if let Some(&first) = items.first() {
                    return self.navigate_to(&format!("{}.{}", cb, first));
                }
            }
        }
        Ok(())
    }

    /// Moves cursor to previous item in active block.
    pub fn previous_item(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            if let Some(block_info) = self.module.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(&cb)) {
                let items: Vec<&str> = block_info.items.iter().map(|i| i.name.as_str()).collect();
                if let Some(curr_item) = self.system.cursor_item.clone() {
                    let item_short = curr_item.split('.').nth(1).unwrap_or(&curr_item);
                    if let Some(pos) = items.iter().position(|&i| i.eq_ignore_ascii_case(item_short)) {
                        let prev_idx = if pos == 0 {
                            items.len().saturating_sub(1)
                        } else {
                            pos - 1
                        };
                        return self.navigate_to(&format!("{}.{}", cb, items[prev_idx]));
                    }
                } else if let Some(&first) = items.first() {
                    return self.navigate_to(&format!("{}.{}", cb, first));
                }
            }
        }
        Ok(())
    }

    /// Moves cursor to specific item.
    pub fn go_item(&mut self, item_path: &str) -> Result<(), RuntimeError> {
        self.navigate_to(item_path)
    }

    /// Moves cursor to first navigable item in target block.
    pub fn go_block(&mut self, block_name: &str) -> Result<(), RuntimeError> {
        let b_upper = block_name.to_ascii_uppercase();
        if let Some(block_info) = self
            .module
            .blocks
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(&b_upper))
        {
            let first_item = block_info
                .items
                .first()
                .map(|i| i.name.as_str())
                .unwrap_or("ITEM1");
            self.navigate_to(&format!("{}.{}", block_info.name, first_item))
        } else {
            Err(RuntimeError::BlockNotFound(block_name.to_string()))
        }
    }

    /// Moves cursor to next record in active block.
    pub fn next_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            let b_upper = cb.to_ascii_uppercase();
            let (can_advance, new_idx) = {
                let buf = self
                    .blocks
                    .get(&b_upper)
                    .ok_or_else(|| RuntimeError::BlockNotFound(cb.clone()))?;
                if buf.current_record_index + 1 < buf.records.len() {
                    (true, buf.current_record_index + 1)
                } else {
                    (false, buf.current_record_index)
                }
            };

            if can_advance {
                self.dispatch_event("POST-RECORD", None, Some(&cb));
                if let Some(buf) = self.blocks.get_mut(&b_upper) {
                    buf.current_record_index = new_idx;
                }
                self.system.cursor_record = new_idx + 1;
                self.dispatch_event("PRE-RECORD", None, Some(&cb));
                self.fire_when_new(&[("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb))]);
                self.update_system_status();
            }
        }
        Ok(())
    }

    /// Moves cursor to previous record in active block.
    pub fn previous_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            let b_upper = cb.to_ascii_uppercase();
            let (can_retreat, new_idx) = {
                let buf = self
                    .blocks
                    .get(&b_upper)
                    .ok_or_else(|| RuntimeError::BlockNotFound(cb.clone()))?;
                if buf.current_record_index > 0 {
                    (true, buf.current_record_index - 1)
                } else {
                    (false, 0)
                }
            };

            if can_retreat {
                self.dispatch_event("POST-RECORD", None, Some(&cb));
                if let Some(buf) = self.blocks.get_mut(&b_upper) {
                    buf.current_record_index = new_idx;
                }
                self.system.cursor_record = new_idx + 1;
                self.dispatch_event("PRE-RECORD", None, Some(&cb));
                self.fire_when_new(&[("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb))]);
                self.update_system_status();
            }
        }
        Ok(())
    }

    /// Moves cursor to first record in active block.
    pub fn first_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            let b_upper = cb.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                if !buf.records.is_empty() {
                    buf.current_record_index = 0;
                    self.system.cursor_record = 1;
                    self.dispatch_event("PRE-RECORD", None, Some(&cb));
                    self.fire_when_new(&[("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb))]);
                    self.update_system_status();
                }
            }
        }
        Ok(())
    }

    /// Moves cursor to last record in active block.
    pub fn last_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            let b_upper = cb.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                if !buf.records.is_empty() {
                    let last_idx = buf.records.len() - 1;
                    buf.current_record_index = last_idx;
                    self.system.cursor_record = last_idx + 1;
                    self.dispatch_event("PRE-RECORD", None, Some(&cb));
                    self.fire_when_new(&[("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb))]);
                    self.update_system_status();
                }
            }
        }
        Ok(())
    }

    /// Moves to record `n` (1-based) in the active block.
    pub fn go_record(&mut self, n: usize) -> Result<(), RuntimeError> {
        let cb = self.active_block().map(str::to_string).ok_or_else(|| RuntimeError::NavigationFailed("no current block".into()))?;
        let b_upper = cb.to_ascii_uppercase();
        let count = self.blocks.get(&b_upper).map(|b| b.records.len()).unwrap_or(0);
        if n == 0 || n > count {
            return Err(RuntimeError::NavigationFailed(format!("FRM-40102: Record {} does not exist", n)));
        }
        self.dispatch_event("POST-RECORD", None, Some(&cb));
        if let Some(buf) = self.blocks.get_mut(&b_upper) {
            buf.current_record_index = n - 1;
        }
        self.system.cursor_record = n;
        self.dispatch_event("PRE-RECORD", None, Some(&cb));
        self.fire_when_new(&[("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb))]);
        self.update_system_status();
        Ok(())
    }

    /// Creates a new record in active block.
    pub fn create_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            let b_upper = cb.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                buf.create_record();
                self.system.cursor_record = buf.current_record_index + 1;
                self.dispatch_event("PRE-RECORD", None, Some(&cb));
                self.fire_when_new(&[("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb))]);
                self.update_system_status();
            }
        }
        Ok(())
    }

    /// Deletes the current record. A queried record is kept aside and deleted from the
    /// database at commit; a new (not yet inserted) record is simply discarded.
    pub fn delete_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            let b_upper = cb.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                buf.delete_record();
                self.system.cursor_record = buf.current_record_index + 1;
                self.update_system_status();
            }
        }
        Ok(())
    }

    /// CLEAR_RECORD: discards changes to the current record.
    pub fn clear_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            if let Some(buf) = self.blocks.get_mut(&cb.to_ascii_uppercase()) {
                buf.clear_record();
            }
            self.update_system_status();
        }
        Ok(())
    }

    /// Builds the block query with runtime overrides and every bind resolved from the
    /// session (`:BLOCK.ITEM`, `:GLOBAL.X`, ...).
    fn build_block_query(&self, block: &BlockInfo, backend: oracle_forms_db::DbBackend, extra_where: Option<&str>) -> Result<GeneratedQuery, RuntimeError> {
        let mut info = block.clone();
        let b_upper = block.name.to_ascii_uppercase();
        let mut dml = info.dml.clone().unwrap_or_default();
        if let Some(props) = self.block_properties.get(&b_upper) {
            if let Some(w) = props.get("DEFAULT_WHERE") {
                dml.where_clause = w.as_text();
            }
            if let Some(o) = props.get("ORDER_BY") {
                dml.order_by_clause = o.as_text();
            }
        }
        if let Some(extra) = extra_where {
            dml.where_clause = Some(match dml.where_clause.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
                Some(w) => format!("({}) AND ({})", w.trim_start_matches("WHERE ").trim_start_matches("where "), extra),
                None => extra.to_string(),
            });
        }
        info.dml = Some(dml);
        let mut generated = SqlGenerator::generate_select(&info, backend, Some(500)).map_err(|e| RuntimeError::Database(e.to_string()))?;
        generated
            .resolve_binds(|reference| Some(self.get_bind_value(reference).to_sql_value()))
            .map_err(|e| RuntimeError::Database(e.to_string()))?;
        Ok(generated)
    }

    /// Runs a generated block query and maps column values onto item names.
    fn fetch_block(&self, driver: &Arc<dyn DatabaseDriver>, generated: &GeneratedQuery, block: &BlockInfo) -> Result<QueryResult, RuntimeError> {
        let mut result = block_on_future(driver.query(&generated.bound_sql)).map_err(|e| RuntimeError::Database(e.to_string()))?;
        result.block_name = block.name.clone();
        result.table_name = generated.table_name.clone();
        for record in &mut result.records {
            for (col, item) in &generated.column_to_item {
                if col != item {
                    if let Some(val) = record.values.get(col).cloned() {
                        record.values.insert(item.to_ascii_uppercase(), val);
                    }
                }
            }
        }
        Ok(result)
    }

    /// Executes query on a block, triggering the full PRE-QUERY -> fetch -> POST-QUERY cascade,
    /// then coordinates detail blocks.
    pub fn execute_query_on_block(&mut self, block_name: &str) -> Result<(), RuntimeError> {
        self.query_block_with_where(block_name, None)
    }

    fn query_block_with_where(&mut self, block_name: &str, extra_where: Option<&str>) -> Result<(), RuntimeError> {
        let b_upper = block_name.to_ascii_uppercase();
        let block_info = self.block_info(&b_upper).cloned().ok_or_else(|| RuntimeError::BlockNotFound(block_name.to_string()))?;

        // 1. PRE-QUERY
        if !self.dispatch_event("PRE-QUERY", None, Some(&block_info.name)).is_success() {
            return Err(RuntimeError::TriggerFailure("PRE-QUERY failed".to_string()));
        }

        // 2. Fetch (errors are reported, never swallowed)
        let driver = self
            .database_driver
            .clone()
            .ok_or_else(|| RuntimeError::Database("EXECUTE_QUERY: no database attached".to_string()))?;
        let generated = self.build_block_query(&block_info, driver.backend(), extra_where)?;
        self.trace_log.push(format!(
            "QUERY: {}{}",
            generated.bound_sql.sql,
            if generated.bound_sql.binds.is_empty() {
                String::new()
            } else {
                format!(
                    "  [binds: {}]",
                    generated.bound_sql.binds.iter().map(|(k, v)| format!("{}={:?}", k, v)).collect::<Vec<_>>().join(", ")
                )
            }
        ));
        let result = self.fetch_block(&driver, &generated, &block_info)?;
        self.system.last_query = Some(generated.bound_sql.sql.clone());
        if let Some(buf) = self.blocks.get_mut(&b_upper) {
            buf.populate_from_query_result(result);
        }

        // 3. POST-QUERY for each fetched record. Values set here are not user changes.
        let rec_count = self.blocks.get(&b_upper).map(|buf| buf.record_count()).unwrap_or(0);
        let fetched = self.blocks.get(&b_upper).map(|b| b.records.iter().any(|r| r.status == FormsRecordStatus::Query)).unwrap_or(false);
        if fetched {
            for idx in 0..rec_count {
                if let Some(buf) = self.blocks.get_mut(&b_upper) {
                    buf.set_cursor(idx);
                }
                self.system.cursor_record = idx + 1;
                if !self.dispatch_event("POST-QUERY", None, Some(&block_info.name)).is_success() {
                    return Err(RuntimeError::TriggerFailure("POST-QUERY failed".to_string()));
                }
                if let Some(rec) = self.blocks.get_mut(&b_upper).and_then(|b| b.records.get_mut(idx)) {
                    rec.reset_to_queried();
                }
            }
        }

        if let Some(buf) = self.blocks.get_mut(&b_upper) {
            buf.set_cursor(0);
        }
        self.system.cursor_record = 1;
        self.update_system_status();

        // Landing on the first fetched record (cursor in this block) fires
        // WHEN-NEW-RECORD-INSTANCE, as in Forms.
        let in_block = self.system.cursor_block.as_deref().map(|b| b.eq_ignore_ascii_case(&block_info.name)).unwrap_or(false);
        if in_block && fetched {
            self.fire_when_new(&[("WHEN-NEW-RECORD-INSTANCE", None, Some(&block_info.name))]);
        }

        // 4. Master-detail coordination
        self.sync_details_blocking(&block_info.name)?;
        Ok(())
    }

    /// Re-queries detail blocks for the master block's current record.
    fn sync_details_blocking(&mut self, master_block: &str) -> Result<(), RuntimeError> {
        let links: Vec<MasterDetailLink> = self
            .master_detail_links
            .iter()
            .filter(|l| l.master_block.eq_ignore_ascii_case(master_block) && l.auto_query && !l.deferred)
            .cloned()
            .collect();
        for link in links {
            let join = link.detail_where_with_binds();
            self.query_block_with_where(&link.detail_block, Some(&join))?;
        }
        Ok(())
    }

    /// Mutates an item's value in the current record, marking it dirty and transitioning
    /// record status. The first change to a queried record locks the row
    /// (`SELECT ... FOR UPDATE NOWAIT`) and checks it has not changed since it was queried.
    pub fn set_item_value(&mut self, item_path: &str, value: serde_json::Value) -> Result<(), RuntimeError> {
        let (b_name, i_name) = match item_path.split_once('.') {
            Some((b, i)) => (b.to_string(), i.to_string()),
            None => (self.active_block().unwrap_or_default().to_string(), item_path.to_string()),
        };
        let b_upper = b_name.to_ascii_uppercase();
        let block_info = self.block_info(&b_upper).cloned();
        // Forms protects items the block or item does not allow to change.
        if let Some(info) = &block_info {
            let status = self.blocks.get(&b_upper).and_then(|b| b.current_record()).map(|r| r.status);
            let item_dml = info.items.iter().find(|i| i.name.eq_ignore_ascii_case(&i_name)).and_then(|i| i.dml.clone());
            let block_dml = info.dml.clone().unwrap_or_default();
            let queried = matches!(status, Some(FormsRecordStatus::Query) | Some(FormsRecordStatus::Changed));
            if queried && (block_dml.update_allowed == Some(false) || item_dml.as_ref().and_then(|d| d.update_allowed) == Some(false)) {
                return Err(RuntimeError::NavigationFailed("FRM-40200: Field is protected against update.".to_string()));
            }
            if !queried && (block_dml.insert_allowed == Some(false) || item_dml.as_ref().and_then(|d| d.insert_allowed) == Some(false)) {
                return Err(RuntimeError::NavigationFailed("FRM-40200: Field is protected against update.".to_string()));
            }
        }
        let (needs_lock, row_id, initial_values) = {
            let buf = self.blocks.get(&b_upper).ok_or_else(|| RuntimeError::BlockNotFound(b_name.clone()))?;
            match buf.current_record() {
                Some(rec) if rec.status == FormsRecordStatus::Query && !rec.is_locked => (true, rec.row_id.clone(), rec.initial_values.clone()),
                _ => (false, None, HashMap::new()),
            }
        };

        if needs_lock && !self.read_only {
            if self.has_trigger("ON-LOCK", &b_name) {
                if !self.dispatch_event("ON-LOCK", None, Some(&b_name)).is_success() {
                    return Err(RuntimeError::RecordLockedOrChanged(
                        "FRM-40501: ORACLE error: unable to reserve record for update or delete.".to_string(),
                    ));
                }
            } else if let (Some(driver), Some(row_id), Some(info)) = (self.database_driver.clone(), row_id, block_info) {
                if driver.supports_transactions() {
                    let bound = SqlGenerator::generate_lock(&info, driver.backend(), &row_id).map_err(|e| RuntimeError::Database(e.to_string()))?;
                    let res = block_on_future(driver.query(&bound)).map_err(|e| {
                        RuntimeError::RecordLockedOrChanged(format!("FRM-40501: ORACLE error: unable to reserve record for update or delete. ({})", e))
                    })?;
                    let Some(db_rec) = res.records.first() else {
                        return Err(RuntimeError::RecordLockedOrChanged("FRM-40652: Cannot lock record: it was deleted by another user".to_string()));
                    };
                    for (col, init_val) in &initial_values {
                        if let Some(db_val) = db_rec.values.get(col) {
                            if db_val != init_val {
                                return Err(RuntimeError::RecordLockedOrChanged(
                                    "FRM-40654: Record has been updated by another user. Re-query to see change.".to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }

        let buf = self.blocks.get_mut(&b_upper).ok_or_else(|| RuntimeError::BlockNotFound(b_name.clone()))?;
        if buf.records.is_empty() {
            buf.create_record();
        }
        if let Some(rec) = buf.current_record_mut() {
            if needs_lock && !self.read_only {
                rec.is_locked = true;
            }
            rec.set_value(&i_name, value);
        }
        self.update_system_status();
        Ok(())
    }

    fn fire_or_fail(&mut self, event: &str, block: Option<&str>) -> Result<(), RuntimeError> {
        match self.dispatch_event(event, None, block) {
            TriggerResult::Success => Ok(()),
            TriggerResult::Failure(m) => Err(RuntimeError::TriggerFailure(format!("{}: {}", event, m))),
            TriggerResult::UnhandledException(m) => Err(RuntimeError::UnhandledException(format!("{}: {}", event, m))),
        }
    }

    /// Validates every changed record: WHEN-VALIDATE-ITEM for changed items, then
    /// WHEN-VALIDATE-RECORD.
    fn validate_for_commit(&mut self) -> Result<(), RuntimeError> {
        let block_names: Vec<String> = self.module.blocks.iter().map(|b| b.name.clone()).collect();
        for b_name in block_names {
            let b_upper = b_name.to_ascii_uppercase();
            let count = self.blocks.get(&b_upper).map(|b| b.records.len()).unwrap_or(0);
            for idx in 0..count {
                // Changed items, in the block's item order (the order Forms validates in).
                let item_order: Vec<String> = self
                    .block_info(&b_upper)
                    .map(|b| b.items.iter().map(|i| i.name.to_ascii_uppercase()).collect())
                    .unwrap_or_default();
                let dirty_items: Vec<String> = match self.blocks.get(&b_upper).and_then(|b| b.records.get(idx)) {
                    Some(rec) if rec.is_dirty() => item_order
                        .iter()
                        .filter(|name| rec.item_validation.get(*name) == Some(&crate::record::ValidationStatus::Changed))
                        .cloned()
                        .collect(),
                    _ => continue,
                };
                if let Some(buf) = self.blocks.get_mut(&b_upper) {
                    buf.set_cursor(idx);
                }
                self.system.cursor_block = Some(b_name.clone());
                self.system.cursor_record = idx + 1;
                for item in dirty_items {
                    match self.dispatch_event("WHEN-VALIDATE-ITEM", Some(&item), Some(&b_name)) {
                        TriggerResult::Success => {
                            if let Some(rec) = self.blocks.get_mut(&b_upper).and_then(|b| b.records.get_mut(idx)) {
                                rec.mark_item_valid(&item);
                            }
                        }
                        TriggerResult::Failure(m) | TriggerResult::UnhandledException(m) => {
                            return Err(RuntimeError::ValidationFailed { item: format!("{}.{}", b_name, item), message: m });
                        }
                    }
                }
                match self.dispatch_event("WHEN-VALIDATE-RECORD", None, Some(&b_name)) {
                    TriggerResult::Success => {}
                    TriggerResult::Failure(m) | TriggerResult::UnhandledException(m) => {
                        return Err(RuntimeError::ValidationFailed { item: format!("{} (record {})", b_name, idx + 1), message: m });
                    }
                }
            }
        }
        Ok(())
    }

    /// Executes the Oracle Forms commit sequence inside a database savepoint:
    ///
    /// 1. Validate changed items and records.
    /// 2. `PRE-COMMIT`.
    /// 3. Per block: deletes first (`PRE-DELETE` → `ON-DELETE` or DELETE → `POST-DELETE`),
    ///    then inserts and updates in record order (`PRE-` → `ON-` or DML → `POST-`).
    /// 4. `POST-FORMS-COMMIT`, then the database COMMIT, then `POST-DATABASE-COMMIT`.
    ///
    /// Any failure rolls the database back to the savepoint and leaves the block buffers
    /// exactly as they were, so the user can fix the problem and commit again.
    pub fn commit(&mut self) -> Result<(), RuntimeError> {
        self.trace_log.push("COMMIT_FORM".to_string());
        let has_changes = self.blocks.iter().any(|(name, b)| {
            self.is_database_block(name) && (!b.deleted_records.is_empty() || b.records.iter().any(|r| r.is_dirty()))
        });
        if !has_changes {
            self.messages.push("FRM-40401: No changes to save.".to_string());
            return Ok(());
        }
        if self.read_only {
            return Err(RuntimeError::ReadOnly("COMMIT_FORM: enable writes for this session to save changes".to_string()));
        }
        let driver = self.database_driver.clone();
        if let Some(d) = &driver {
            if !d.supports_transactions() {
                return Err(RuntimeError::Unsupported(format!(
                    "{:?} driver cannot run transactions; use the Oracle thin or SQLite backend to save changes",
                    d.backend()
                )));
            }
        }

        self.validate_for_commit()?;

        // Snapshot buffers so a failed commit can restore them exactly.
        let snapshot: HashMap<String, BlockBuffer> = self.blocks.clone();
        const SAVEPOINT: &str = "OFW_COMMIT";
        if let Some(d) = &driver {
            block_on_future(d.savepoint(SAVEPOINT)).map_err(|e| RuntimeError::Database(e.to_string()))?;
        }

        let result = self.commit_inner(driver.as_ref());
        match result {
            Ok(()) => {
                if let Some(d) = &driver {
                    if let Err(e) = block_on_future(d.commit()) {
                        let _ = block_on_future(d.rollback_to_savepoint(SAVEPOINT));
                        self.blocks = snapshot;
                        self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                        return Err(RuntimeError::Database(format!("COMMIT failed: {}", e)));
                    }
                }
                self.trace_log.push("DATABASE_COMMIT".to_string());
                let db_blocks: Vec<String> = self.blocks.keys().filter(|n| self.is_database_block(n)).cloned().collect();
                for name in db_blocks {
                    let Some(buf) = self.blocks.get_mut(&name) else { continue };
                    buf.deleted_records.clear();
                    for rec in &mut buf.records {
                        if rec.is_dirty() {
                            rec.status = FormsRecordStatus::Query;
                            rec.initial_values = rec.values.clone();
                            rec.mark_record_valid();
                            rec.is_locked = false;
                        }
                    }
                }
                self.update_system_status();
                self.messages.push("FRM-40400: Transaction complete.".to_string());
                let _ = self.dispatch_event("POST-DATABASE-COMMIT", None, None);
                Ok(())
            }
            Err(e) => {
                if let Some(d) = &driver {
                    if let Err(rb) = block_on_future(d.rollback_to_savepoint(SAVEPOINT)) {
                        self.trace_log.push(format!("ROLLBACK_FAILED: {}", rb));
                    }
                }
                self.blocks = snapshot;
                self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                self.update_system_status();
                Err(e)
            }
        }
    }

    fn run_dml(&mut self, driver: Option<&Arc<dyn DatabaseDriver>>, stmt: oracle_forms_db::BoundSql, expect_row: bool, what: &str) -> Result<(), RuntimeError> {
        let Some(d) = driver else {
            return Err(RuntimeError::Database(format!("{}: no database attached", what)));
        };
        self.trace_log.push(format!("DML: {}", stmt.sql));
        let rows = block_on_future(d.execute(&stmt)).map_err(|e| RuntimeError::Database(format!("{} failed: {}", what, e)))?;
        if expect_row && rows == 0 {
            return Err(RuntimeError::RecordLockedOrChanged(format!(
                "{}: no row matched (the record was changed or deleted by another user)",
                what
            )));
        }
        Ok(())
    }

    fn commit_inner(&mut self, driver: Option<&Arc<dyn DatabaseDriver>>) -> Result<(), RuntimeError> {
        self.fire_or_fail("PRE-COMMIT", None)?;

        let block_names: Vec<String> = self.module.blocks.iter().map(|b| b.name.clone()).collect();
        for b_name in block_names {
            let b_upper = b_name.to_ascii_uppercase();
            if !self.is_database_block(&b_upper) {
                continue;
            }
            let Some(info) = self.block_info(&b_upper).cloned() else { continue };
            let backend = driver.map(|d| d.backend()).unwrap_or_default();

            // Deletes first.
            let deleted: Vec<crate::record::FormRecord> = self.blocks.get(&b_upper).map(|b| b.deleted_records.clone()).unwrap_or_default();
            for rec in deleted {
                // Make the deleted record current so triggers can read its values.
                if let Some(buf) = self.blocks.get_mut(&b_upper) {
                    buf.records.push(rec.clone());
                    buf.current_record_index = buf.records.len() - 1;
                }
                let outcome = (|| -> Result<(), RuntimeError> {
                    self.fire_or_fail("PRE-DELETE", Some(&b_name))?;
                    if self.has_trigger("ON-DELETE", &b_name) {
                        self.fire_or_fail("ON-DELETE", Some(&b_name))?;
                    } else {
                        let row_id = rec.row_id.clone().ok_or_else(|| RuntimeError::Database(format!("{}: deleted record has no ROWID", b_name)))?;
                        let stmt = SqlGenerator::generate_delete(&info, backend, &row_id).map_err(|e| RuntimeError::Database(e.to_string()))?;
                        self.run_dml(driver, stmt, true, &format!("DELETE from {}", b_name))?;
                    }
                    self.fire_or_fail("POST-DELETE", Some(&b_name))
                })();
                if let Some(buf) = self.blocks.get_mut(&b_upper) {
                    buf.records.pop();
                    buf.current_record_index = buf.current_record_index.min(buf.records.len().saturating_sub(1));
                }
                outcome?;
            }

            // Inserts and updates, in record order.
            let count = self.blocks.get(&b_upper).map(|b| b.records.len()).unwrap_or(0);
            for idx in 0..count {
                let status = self.blocks.get(&b_upper).and_then(|b| b.records.get(idx)).map(|r| r.status);
                let is_insert = status == Some(FormsRecordStatus::Insert);
                let is_update = status == Some(FormsRecordStatus::Changed);
                if !is_insert && !is_update {
                    continue;
                }
                if let Some(buf) = self.blocks.get_mut(&b_upper) {
                    buf.set_cursor(idx);
                }
                self.system.cursor_block = Some(b_name.clone());
                self.system.cursor_record = idx + 1;

                let (pre, on, post) = if is_insert {
                    ("PRE-INSERT", "ON-INSERT", "POST-INSERT")
                } else {
                    ("PRE-UPDATE", "ON-UPDATE", "POST-UPDATE")
                };
                self.fire_or_fail(pre, Some(&b_name))?;
                if self.has_trigger(on, &b_name) {
                    self.fire_or_fail(on, Some(&b_name))?;
                } else {
                    // Re-read values: PRE- triggers commonly assign keys from sequences.
                    let rec = self.blocks.get(&b_upper).and_then(|b| b.records.get(idx)).cloned().expect("record exists");
                    if is_insert {
                        let stmt = SqlGenerator::generate_insert(&info, backend, &rec.values).map_err(|e| RuntimeError::Database(e.to_string()))?;
                        self.run_dml(driver, stmt, true, &format!("INSERT into {}", b_name))?;
                    } else {
                        let row_id = rec.row_id.clone().ok_or_else(|| RuntimeError::Database(format!("{}: changed record has no ROWID", b_name)))?;
                        let stmt = SqlGenerator::generate_update(&info, backend, &rec.values, &row_id).map_err(|e| RuntimeError::Database(e.to_string()))?;
                        self.run_dml(driver, stmt, true, &format!("UPDATE of {}", b_name))?;
                    }
                }
                self.fire_or_fail(post, Some(&b_name))?;
            }
        }

        self.fire_or_fail("POST-FORMS-COMMIT", None)
    }

    /// Queries the block using the provided database driver and synchronizes master-detail relations.
    pub async fn execute_query(&mut self, block_name: &str, driver: Option<&dyn DatabaseDriver>) -> Result<QueryResult, RuntimeError> {
        let b_upper = block_name.to_ascii_uppercase();
        let block_info = self.block_info(block_name).cloned().ok_or_else(|| RuntimeError::BlockNotFound(block_name.to_string()))?;
        let drv = driver.ok_or_else(|| RuntimeError::Database("execute_query: no database driver supplied".to_string()))?;

        if !self.dispatch_event("PRE-QUERY", None, Some(block_name)).is_success() {
            return Err(RuntimeError::TriggerFailure("PRE-QUERY failed".to_string()));
        }
        let generated = self.build_block_query(&block_info, drv.backend(), None)?;
        let mut query_result = drv.query(&generated.bound_sql).await.map_err(|e| RuntimeError::Database(e.to_string()))?;
        query_result.block_name = block_info.name.clone();
        query_result.table_name = generated.table_name.clone();
        for record in &mut query_result.records {
            for (col, item) in &generated.column_to_item {
                if col != item {
                    if let Some(val) = record.values.get(col).cloned() {
                        record.values.insert(item.to_ascii_uppercase(), val);
                    }
                }
            }
        }
        self.system.last_query = Some(query_result.sql_statement.clone());

        if let Some(buf) = self.blocks.get_mut(&b_upper) {
            buf.populate_from_query_result(query_result.clone());
        }
        let count = self.blocks.get(&b_upper).map(|b| b.records.len()).unwrap_or(0);
        for idx in 0..count {
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                buf.set_cursor(idx);
            }
            self.system.cursor_record = idx + 1;
            self.dispatch_event("POST-QUERY", None, Some(block_name));
            if let Some(rec) = self.blocks.get_mut(&b_upper).and_then(|b| b.records.get_mut(idx)) {
                rec.reset_to_queried();
            }
        }
        if let Some(buf) = self.blocks.get_mut(&b_upper) {
            buf.set_cursor(0);
        }

        self.synchronize_details(block_name, driver).await?;
        self.update_system_status();
        Ok(query_result)
    }

    /// Synchronizes detail blocks when master block's active row changes. Master values
    /// are passed as bind parameters.
    pub async fn synchronize_details(&mut self, master_block_name: &str, driver: Option<&dyn DatabaseDriver>) -> Result<(), RuntimeError> {
        let links: Vec<MasterDetailLink> = self
            .master_detail_links
            .iter()
            .filter(|l| l.master_block.eq_ignore_ascii_case(master_block_name) && l.auto_query)
            .cloned()
            .collect();
        let Some(drv) = driver else { return Ok(()) };

        for link in links {
            let Some(detail_block) = self.block_info(&link.detail_block).cloned() else { continue };
            let join = link.detail_where_with_binds();
            let generated = {
                let mut info = detail_block.clone();
                let mut dml = info.dml.clone().unwrap_or_default();
                dml.where_clause = Some(match dml.where_clause.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
                    Some(w) => format!("({}) AND ({})", w, join),
                    None => join.clone(),
                });
                info.dml = Some(dml);
                let mut g = SqlGenerator::generate_select(&info, drv.backend(), Some(500)).map_err(|e| RuntimeError::Database(e.to_string()))?;
                g.resolve_binds(|reference| Some(self.get_bind_value(reference).to_sql_value()))
                    .map_err(|e| RuntimeError::Database(e.to_string()))?;
                g
            };
            let res = drv.query(&generated.bound_sql).await.map_err(|e| RuntimeError::Database(e.to_string()))?;
            if let Some(buf) = self.blocks.get_mut(&link.detail_block.to_ascii_uppercase()) {
                buf.populate_from_query_result(res);
            }
        }
        Ok(())
    }

    /// Synchronizes aggregate status in SystemVariables.
    fn update_system_status(&mut self) {
        if let Some(cb) = &self.system.cursor_block {
            let cb_upper = cb.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get(&cb_upper) {
                if let Some(rec) = buf.current_record() {
                    self.system.record_status = rec.status;
                }
                self.system.block_status = buf.status();
            }
        }

        let is_changed = self
            .blocks
            .values()
            .any(|b| b.status() == FormsRecordStatus::Changed);
        let is_insert = self
            .blocks
            .values()
            .any(|b| b.status() == FormsRecordStatus::Insert);
        let is_query = self
            .blocks
            .values()
            .any(|b| b.status() == FormsRecordStatus::Query);

        self.system.form_status = if is_changed {
            FormsRecordStatus::Changed
        } else if is_insert {
            FormsRecordStatus::Insert
        } else if is_query {
            FormsRecordStatus::Query
        } else {
            FormsRecordStatus::New
        };
    }
}
