use crate::interpreter::{ExecutionStatus, InteractionResponse, Interpreter, SuspensionContext};
use crate::master_detail::MasterDetailLink;
use crate::record::{BlockBuffer, FormsRecordStatus};
use crate::system::{OperationalMode, SystemVariables};
use crate::trigger::{ExecutionHierarchy, ResolvedTrigger, TriggerResolver, TriggerResult};
use crate::value::OracleValue;
use oracle_forms_core::model::ModuleIndex;
use oracle_forms_db::{DatabaseDriver, QueryResult};
use oracle_forms_plsql::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
}

pub type ValidationHook =
    Box<dyn Fn(&str, &str, &serde_json::Value) -> Result<(), String> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertDefinition {
    pub name: String,
    pub message: String,
    pub button1: String,
    pub button2: Option<String>,
    pub button3: Option<String>,
    pub default_button: u8,
}

pub(crate) fn block_on_future<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(fut))
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create temporary tokio runtime");
        rt.block_on(fut)
    }
}

/// Active in-memory Form runtime session.
pub struct FormSession {
    pub module: ModuleIndex,
    pub blocks: HashMap<String, BlockBuffer>,
    pub system: SystemVariables,
    pub master_detail_links: Vec<MasterDetailLink>,
    pub trace_log: Vec<String>,
    pub hierarchy: ExecutionHierarchy,
    pub validation_hook: Option<ValidationHook>,
    pub global_variables: HashMap<String, OracleValue>,
    pub messages: Vec<String>,
    pub alerts: HashMap<String, AlertDefinition>,
    pub item_properties: HashMap<String, HashMap<String, OracleValue>>,
    pub database_driver: Option<Arc<dyn DatabaseDriver>>,
    pub pending_suspension: Option<SuspensionContext>,
    pub interactive_mode: bool,
    pub sequences: std::sync::Mutex<HashMap<String, u64>>,
}

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
                    message: a
                        .message
                        .clone()
                        .unwrap_or_else(|| format!("Alert: {}", a.name)),
                    button1: "OK".to_string(),
                    button2: Some("Cancel".to_string()),
                    button3: None,
                    default_button: 1,
                },
            );
        }

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
            database_driver: None,
            pending_suspension: None,
            interactive_mode: false,
            sequences: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Sets custom validation hook for simulating trigger validation logic.
    pub fn set_validation_hook(&mut self, hook: ValidationHook) {
        self.validation_hook = Some(hook);
    }

    /// Sets attached database driver for embedded SQL and query execution.
    pub fn set_database_driver(&mut self, driver: Arc<dyn DatabaseDriver>) {
        self.database_driver = Some(driver);
    }

    /// Returns the currently active block name, or default if unset.
    pub fn active_block(&self) -> Option<&str> {
        self.system.cursor_block.as_deref().or_else(|| {
            self.module.blocks.first().map(|b| b.name.as_str())
        })
    }

    /// Gets a bind variable value (`:BLOCK.ITEM`, `:SYSTEM.*`, `:GLOBAL.*`).
    pub fn get_bind_value(&self, bind: &str) -> OracleValue {
        let clean = bind.trim().trim_start_matches(':').to_ascii_uppercase();

        if clean.starts_with("SYSTEM.") {
            let var_name = clean.trim_start_matches("SYSTEM.");
            return self
                .system
                .get_var(var_name)
                .map(OracleValue::varchar2)
                .unwrap_or(OracleValue::Null);
        }

        if clean.starts_with("GLOBAL.") {
            let var_name = clean.trim_start_matches("GLOBAL.");
            return self
                .global_variables
                .get(var_name)
                .cloned()
                .unwrap_or(OracleValue::Null);
        }

        if clean.starts_with("SEQUENCE.") {
            let seq_part = clean.trim_start_matches("SEQUENCE.");
            let seq_name = seq_part.split('.').next().unwrap_or(seq_part);
            let seq_name = seq_name.to_ascii_uppercase();

            let mut lock = self.sequences.lock().unwrap();
            let entry = lock.entry(seq_name).or_insert(1000);
            *entry += 1;
            return OracleValue::Number(*entry as f64);
        }

        if clean.contains('.') {
            let parts: Vec<&str> = clean.splitn(2, '.').collect();
            let block_name = parts[0];
            let item_name = parts[1];
            if let Some(buf) = self.blocks.get(block_name) {
                if let Some(rec) = buf.current_record() {
                    if let Some(val) = rec.get_value(item_name) {
                        return OracleValue::from_json(val);
                    }
                }
            }
        } else if let Some(active) = self.active_block() {
            let active_upper = active.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get(&active_upper) {
                if let Some(rec) = buf.current_record() {
                    if let Some(val) = rec.get_value(&clean) {
                        return OracleValue::from_json(val);
                    }
                }
            }
        }

        OracleValue::Null
    }

    /// Sets a bind variable value (`:BLOCK.ITEM`, `:SYSTEM.*`, `:GLOBAL.*`).
    pub fn set_bind_value(&mut self, bind: &str, val: OracleValue) {
        let clean = bind.trim().trim_start_matches(':').to_ascii_uppercase();

        if clean.starts_with("SYSTEM.") {
            let var_name = clean.trim_start_matches("SYSTEM.");
            if let Some(s) = val.as_str() {
                self.system.set_var(var_name, s);
            }
            return;
        }

        if clean.starts_with("GLOBAL.") {
            let var_name = clean.trim_start_matches("GLOBAL.").to_string();
            self.global_variables.insert(var_name, val);
            return;
        }

        if clean.contains('.') {
            let parts: Vec<&str> = clean.splitn(2, '.').collect();
            let block_name = parts[0];
            let item_name = parts[1];
            if let Some(buf) = self.blocks.get_mut(block_name) {
                if let Some(rec) = buf.current_record_mut() {
                    rec.set_value(item_name, val.to_json());
                }
            }
        } else if let Some(active) = self.active_block() {
            let active_upper = active.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get_mut(&active_upper) {
                if let Some(rec) = buf.current_record_mut() {
                    rec.set_value(&clean, val.to_json());
                }
            }
        }
    }

    /// Erases a global session variable (`ERASE('GLOBAL.X')`).
    pub fn erase_global(&mut self, name: &str) {
        let clean = name.trim().trim_start_matches(':').trim_start_matches("GLOBAL.").to_ascii_uppercase();
        self.global_variables.remove(&clean);
    }

    /// Gets an item property.
    pub fn get_item_property(&self, item: &str, prop: &str) -> OracleValue {
        let item_upper = item.to_ascii_uppercase();
        let prop_upper = prop.to_ascii_uppercase();
        self.item_properties
            .get(&item_upper)
            .and_then(|p| p.get(&prop_upper))
            .cloned()
            .unwrap_or(OracleValue::Null)
    }

    /// Sets an item property.
    pub fn set_item_property(&mut self, item: &str, prop: &str, val: OracleValue) {
        let item_upper = item.to_ascii_uppercase();
        let prop_upper = prop.to_ascii_uppercase();
        self.item_properties
            .entry(item_upper)
            .or_default()
            .insert(prop_upper, val);
    }

    /// Clears records in a block.
    pub fn clear_block(&mut self, block_name: &str) {
        let b_upper = block_name.to_ascii_uppercase();
        if let Some(buf) = self.blocks.get_mut(&b_upper) {
            buf.records.clear();
            buf.current_record_index = 0;
        }
        self.update_system_status();
    }

    /// Clears all blocks in the form.
    pub fn clear_form(&mut self) {
        for buf in self.blocks.values_mut() {
            buf.records.clear();
            buf.current_record_index = 0;
        }
        self.update_system_status();
    }

    /// Emits and records a lifecycle or trigger event in the session trace log.
    pub fn dispatch_event(
        &mut self,
        event_name: &str,
        item: Option<&str>,
        block: Option<&str>,
    ) -> TriggerResult {
        let candidates: Vec<ResolvedTrigger> = TriggerResolver::resolve(
            event_name,
            item,
            block,
            &self.module.triggers,
            self.hierarchy,
        );

        for trigger in candidates {
            let trace_entry = match (&trigger.block_name, &trigger.item_name) {
                (Some(b), Some(i)) => format!("{}({}.{})", trigger.name, b, i),
                (Some(b), None) => format!("{}({})", trigger.name, b),
                _ => trigger.name.clone(),
            };
            self.trace_log.push(trace_entry);

            // Execute PL/SQL if trigger has source text
            if let Some(trig_info) = &trigger.trigger_info {
                let code = trig_info
                    .source_text
                    .as_deref()
                    .unwrap_or(&trig_info.first_line);
                if !code.trim().is_empty() {
                    let mut parser = Parser::new(code);
                    if let Ok(ast) = parser.parse_unit() {
                        let mut interpreter = Interpreter::new(self.interactive_mode);
                        match interpreter.execute_unit(&ast, self) {
                            Ok(ExecutionStatus::TriggerFailure(msg)) => {
                                self.trace_log.push(format!("TRIGGER_FAILURE: {}", msg));
                                return TriggerResult::Failure(msg);
                            }
                            Ok(ExecutionStatus::UnhandledException(ex)) => {
                                self.trace_log.push(format!("UNHANDLED_EXCEPTION: {}", ex));
                                return TriggerResult::UnhandledException(ex);
                            }
                            Ok(ExecutionStatus::Suspended(ctx)) => {
                                self.pending_suspension = Some(*ctx);
                                self.trace_log.push("SUSPENDED_FOR_INTERACTION".to_string());
                            }
                            Ok(ExecutionStatus::Success(_)) => {}
                            Err(e) => {
                                self.trace_log.push(format!("RUNTIME_ERROR: {}", e));
                                return TriggerResult::UnhandledException(e.to_string());
                            }
                        }
                    }
                }
            }

            // If this is WHEN-VALIDATE-ITEM and a validation hook is provided, evaluate it
            if event_name.eq_ignore_ascii_case("WHEN-VALIDATE-ITEM") {
                if let (Some(b_name), Some(i_name)) = (block, item) {
                    if let Some(hook) = &self.validation_hook {
                        let b_upper = b_name.to_ascii_uppercase();
                        if let Some(buf) = self.blocks.get(&b_upper) {
                            if let Some(rec) = buf.current_record() {
                                let val =
                                    rec.get_value(i_name).unwrap_or(&serde_json::Value::Null);
                                if let Err(err_msg) = hook(b_name, i_name, val) {
                                    self.trace_log.push(format!("TRIGGER_FAILURE: {}", err_msg));
                                    return TriggerResult::Failure(err_msg);
                                }
                            }
                        }
                    }
                }
            }
        }

        TriggerResult::Success
    }

    /// Resumes a suspended interactive session with user response.
    pub fn resume(
        &mut self,
        response: InteractionResponse,
    ) -> Result<ExecutionStatus, RuntimeError> {
        if let Some(ctx) = self.pending_suspension.take() {
            let mut interpreter = Interpreter::new(self.interactive_mode);
            let status = interpreter.resume(self, ctx, response)?;
            match &status {
                ExecutionStatus::Suspended(new_ctx) => {
                    self.pending_suspension = Some((**new_ctx).clone());
                }
                ExecutionStatus::TriggerFailure(f) => {
                    self.trace_log.push(format!("TRIGGER_FAILURE: {}", f));
                }
                ExecutionStatus::UnhandledException(ex) => {
                    self.trace_log.push(format!("UNHANDLED_EXCEPTION: {}", ex));
                }
                ExecutionStatus::Success(_) => {}
            }
            Ok(status)
        } else {
            Ok(ExecutionStatus::Success(None))
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
            .unwrap_or_else(|| "BLOCK1".to_string());

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
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "ITEM1".to_string());

        self.system.cursor_block = Some(first_block.clone());
        self.system.cursor_item = Some(format!("{}.{}", first_block, first_item));
        self.system.cursor_record = 1;
        self.system.record_status = FormsRecordStatus::New;
        self.system.block_status = FormsRecordStatus::New;
        self.system.form_status = FormsRecordStatus::New;
        self.system.mode = OperationalMode::Normal;

        // Startup lifecycle event cascade
        self.dispatch_event("PRE-FORM", None, None);
        self.dispatch_event("PRE-BLOCK", None, Some(&first_block));
        self.dispatch_event("PRE-RECORD", None, Some(&first_block));
        self.dispatch_event("PRE-TEXT-ITEM", Some(&first_item), Some(&first_block));
        self.dispatch_event("WHEN-NEW-FORM-INSTANCE", None, None);
        self.dispatch_event("WHEN-NEW-BLOCK-INSTANCE", None, Some(&first_block));
        self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(&first_block));
        self.dispatch_event("WHEN-NEW-ITEM-INSTANCE", Some(&first_item), Some(&first_block));

        Ok(())
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
        let is_block_change = curr_block_opt.as_deref() != Some(&target_block);
        if is_block_change {
            if let Some(cb) = &curr_block_opt {
                self.dispatch_event("POST-BLOCK", None, Some(cb));
            }
            self.dispatch_event("PRE-BLOCK", None, Some(&target_block));
        }

        // 3. Enter target item triggers
        self.dispatch_event("PRE-TEXT-ITEM", Some(&target_item), Some(&target_block));
        self.dispatch_event("WHEN-NEW-ITEM-INSTANCE", Some(&target_item), Some(&target_block));

        // Update cursor focus in system variables
        self.system.cursor_block = Some(target_block.clone());
        self.system.cursor_item = Some(format!("{}.{}", target_block, target_item));
        self.update_system_status();

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
                self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb));
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
                self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb));
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
                    self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb));
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
                    self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb));
                    self.update_system_status();
                }
            }
        }
        Ok(())
    }

    /// Creates a new record in active block.
    pub fn create_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            let b_upper = cb.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                buf.create_record();
                self.system.cursor_record = buf.records.len();
                self.dispatch_event("PRE-RECORD", None, Some(&cb));
                self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(&cb));
                self.update_system_status();
            }
        }
        Ok(())
    }

    /// Deletes the current record in active block.
    pub fn delete_record(&mut self) -> Result<(), RuntimeError> {
        if let Some(cb) = self.active_block().map(|s| s.to_string()) {
            let b_upper = cb.to_ascii_uppercase();
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                if !buf.records.is_empty() {
                    buf.records.remove(buf.current_record_index);
                    if buf.current_record_index >= buf.records.len() && !buf.records.is_empty() {
                        buf.current_record_index = buf.records.len() - 1;
                    }
                    self.system.cursor_record = buf.current_record_index + 1;
                    self.update_system_status();
                }
            }
        }
        Ok(())
    }

    /// Executes query on a block, triggering the full PRE-QUERY -> fetch -> POST-QUERY cascade.
    pub fn execute_query_on_block(&mut self, block_name: &str) -> Result<(), RuntimeError> {
        let b_upper = block_name.to_ascii_uppercase();

        // 1. Fire PRE-QUERY
        let pre_res = self.dispatch_event("PRE-QUERY", None, Some(block_name));
        if !pre_res.is_success() {
            return Err(RuntimeError::TriggerFailure("PRE-QUERY failed".to_string()));
        }

        // 2. Query execution if database driver attached
        if let Some(driver) = self.database_driver.clone() {
            if let Some(block_info) = self
                .module
                .blocks
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(&b_upper))
                .cloned()
            {
                let query_fut = driver.query_block(&block_info, Some(50));
                let result = block_on_future(query_fut);
                if let Ok(qr) = result {
                    if let Some(buf) = self.blocks.get_mut(&b_upper) {
                        buf.populate_from_query_result(qr);
                    }
                }
            }
        }

        // 3. Fire POST-QUERY for each record in the block
        let rec_count = self
            .blocks
            .get(&b_upper)
            .map(|buf| buf.record_count())
            .unwrap_or(0);

        for idx in 0..rec_count {
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                buf.set_cursor(idx);
            }
            self.system.cursor_record = idx + 1;
            let post_res = self.dispatch_event("POST-QUERY", None, Some(block_name));
            if !post_res.is_success() {
                return Err(RuntimeError::TriggerFailure("POST-QUERY failed".to_string()));
            }
        }

        if rec_count > 0 {
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                buf.set_cursor(0);
            }
            self.system.cursor_record = 1;
        }

        self.update_system_status();
        Ok(())
    }

    /// Mutates an item's value in the current record, marking it dirty and transitioning record status.
    pub fn set_item_value(
        &mut self,
        item_path: &str,
        value: serde_json::Value,
    ) -> Result<(), RuntimeError> {
        let (b_name, i_name) = if let Some((b, i)) = item_path.split_once('.') {
            (b.to_string(), i.to_string())
        } else {
            let active_b = self
                .system
                .cursor_block
                .clone()
                .unwrap_or_else(|| "BLOCK1".to_string());
            (active_b, item_path.to_string())
        };

        let b_upper = b_name.to_ascii_uppercase();
        let block_info_opt = self
            .module
            .blocks
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(&b_upper))
            .cloned();

        // 1. Genuine Forms Locking on First Edit:
        let (needs_lock, row_id_opt, initial_values_opt) = {
            let buf = self
                .blocks
                .get(&b_upper)
                .ok_or_else(|| RuntimeError::BlockNotFound(b_name.clone()))?;
            if let Some(rec) = buf.current_record() {
                if rec.status == FormsRecordStatus::Query && !rec.is_locked {
                    (true, rec.row_id.clone(), Some(rec.initial_values.clone()))
                } else {
                    (false, None, None)
                }
            } else {
                (false, None, None)
            }
        };

        if needs_lock {
            // Check if ON-LOCK trigger defined on the block
            let on_lock_exists = self.module.triggers.iter().any(|t| {
                t.name.eq_ignore_ascii_case("ON-LOCK")
                    && t.block_name
                        .as_deref()
                        .map(|b| b.eq_ignore_ascii_case(&b_upper))
                        .unwrap_or(false)
            });

            if on_lock_exists {
                let lock_res = self.dispatch_event("ON-LOCK", None, Some(&b_name));
                if !lock_res.is_success() {
                    return Err(RuntimeError::RecordLockedOrChanged(
                        "FRM-40501: ORACLE error: unable to reserve record for update or delete."
                            .to_string(),
                    ));
                }
            } else if let (Some(driver), Some(row_id), Some(b_info), Some(initial_vals)) =
                (&self.database_driver, &row_id_opt, &block_info_opt, &initial_values_opt)
            {
                // Generate lock SQL (SELECT ... FOR UPDATE NOWAIT)
                if let Ok(bound) =
                    oracle_forms_db::SqlGenerator::generate_lock(b_info, driver.backend(), row_id)
                {
                    let query_fut = driver.query(&bound);
                    let result = block_on_future(query_fut);

                    match result {
                        Ok(res) => {
                            // Concurrency re-check: compare returned columns against initial_values
                            if let Some(db_rec) = res.records.first() {
                                for (col, init_val) in initial_vals {
                                    if let Some(db_val) = db_rec.values.get(col) {
                                        if db_val != init_val {
                                            return Err(RuntimeError::RecordLockedOrChanged(
                                                "FRM-40501: ORACLE error: unable to reserve record for update or delete."
                                                    .to_string(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            return Err(RuntimeError::RecordLockedOrChanged(format!(
                                "FRM-40501: ORACLE error: unable to reserve record for update or delete. ({})",
                                e
                            )));
                        }
                    }
                }
            }
        }

        let buf = self
            .blocks
            .get_mut(&b_upper)
            .ok_or_else(|| RuntimeError::BlockNotFound(b_name.clone()))?;

        if let Some(rec) = buf.current_record_mut() {
            if rec.status == FormsRecordStatus::Query {
                rec.is_locked = true;
                rec.status = FormsRecordStatus::Changed;
            } else if rec.status == FormsRecordStatus::New {
                rec.status = FormsRecordStatus::Insert;
            }
            rec.set_value(&i_name, value);
        } else {
            buf.create_record();
            if let Some(rec) = buf.current_record_mut() {
                rec.status = FormsRecordStatus::Insert;
                rec.set_value(&i_name, value);
            }
        }

        self.update_system_status();
        Ok(())
    }

    /// Executes the full Oracle Forms Commit Event Pipeline:
    ///
    /// 1. `KEY-COMMIT` -> Validation -> `PRE-COMMIT`
    /// 2. For each dirty record:
    ///    - If Marked for Delete: `PRE-DELETE` -> `ON-DELETE` (or SQL DELETE) -> `POST-DELETE`
    ///    - If Insert: `PRE-INSERT` -> `ON-INSERT` (or SQL INSERT) -> `POST-INSERT`
    ///    - If Changed: `PRE-UPDATE` -> `ON-UPDATE` (or SQL UPDATE) -> `POST-UPDATE`
    /// 3. `POST-FORMS-COMMIT` -> Database `COMMIT` (or savepoint rollback on error)
    pub fn commit(&mut self) -> Result<(), RuntimeError> {
        self.trace_log.push("COMMIT_FORM".to_string());

        // 1. Validation across all blocks
        for (b_name, buf) in &self.blocks {
            for rec in &buf.records {
                if rec.validation_status == crate::record::ValidationStatus::Changed {
                    if let Some(hook) = &self.validation_hook {
                        for (item, val) in &rec.values {
                            if let Err(err_msg) = hook(b_name, item, val) {
                                self.trace_log.push(format!("VALIDATION_FAILED: {}", err_msg));
                                return Err(RuntimeError::ValidationFailed {
                                    item: format!("{}.{}", b_name, item),
                                    message: err_msg,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 2. PRE-COMMIT
        let pre_commit_res = self.dispatch_event("PRE-COMMIT", None, None);
        if !pre_commit_res.is_success() {
            self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
            return Err(RuntimeError::TriggerFailure("PRE-COMMIT failed".to_string()));
        }

        // 3. Process each dirty record in each block
        let block_names: Vec<String> = self.module.blocks.iter().map(|b| b.name.clone()).collect();
        for b_name in block_names {
            let b_upper = b_name.to_ascii_uppercase();
            let b_info = self
                .module
                .blocks
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(&b_name))
                .cloned();

            let rec_count = self
                .blocks
                .get(&b_upper)
                .map(|b| b.records.len())
                .unwrap_or(0);

            for rec_idx in 0..rec_count {
                // Set cursor to active record
                if let Some(buf) = self.blocks.get_mut(&b_upper) {
                    buf.set_cursor(rec_idx);
                }
                self.system.cursor_block = Some(b_name.clone());
                self.system.cursor_record = rec_idx + 1;

                let (is_delete, is_insert, is_update, row_id_opt, values_clone) = {
                    let buf = self.blocks.get(&b_upper).unwrap();
                    let rec = &buf.records[rec_idx];
                    (
                        rec.is_marked_for_delete,
                        rec.status == FormsRecordStatus::Insert,
                        rec.status == FormsRecordStatus::Changed,
                        rec.row_id.clone(),
                        rec.values.clone(),
                    )
                };

                // Case A: Deletions
                if is_delete {
                    let pre_del = self.dispatch_event("PRE-DELETE", None, Some(&b_name));
                    if !pre_del.is_success() {
                        self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                        return Err(RuntimeError::TriggerFailure("PRE-DELETE failed".to_string()));
                    }

                    // Check ON-DELETE trigger
                    let on_del_exists = self.module.triggers.iter().any(|t| {
                        t.name.eq_ignore_ascii_case("ON-DELETE")
                            && t.block_name
                                .as_deref()
                                .map(|b| b.eq_ignore_ascii_case(&b_upper))
                                .unwrap_or(false)
                    });

                    if on_del_exists {
                        let on_del = self.dispatch_event("ON-DELETE", None, Some(&b_name));
                        if !on_del.is_success() {
                            self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                            return Err(RuntimeError::TriggerFailure("ON-DELETE failed".to_string()));
                        }
                    } else if let (Some(driver), Some(row_id), Some(b_meta)) =
                        (&self.database_driver, &row_id_opt, &b_info)
                    {
                        if let Ok(del_sql) =
                            oracle_forms_db::SqlGenerator::generate_delete(b_meta, driver.backend(), row_id)
                        {
                            let del_fut = driver.query(&del_sql);
                            let _ = block_on_future(del_fut);
                        }
                    }

                    let post_del = self.dispatch_event("POST-DELETE", None, Some(&b_name));
                    if !post_del.is_success() {
                        self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                        return Err(RuntimeError::TriggerFailure("POST-DELETE failed".to_string()));
                    }
                }
                // Case B: Inserts
                else if is_insert {
                    let pre_ins = self.dispatch_event("PRE-INSERT", None, Some(&b_name));
                    if !pre_ins.is_success() {
                        self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                        return Err(RuntimeError::TriggerFailure("PRE-INSERT failed".to_string()));
                    }

                    // Re-read values in case PRE-INSERT mutated them (e.g. Assigned primary key sequence)
                    let current_values = self
                        .blocks
                        .get(&b_upper)
                        .map(|b| b.records[rec_idx].values.clone())
                        .unwrap_or(values_clone);

                    // Check ON-INSERT trigger
                    let on_ins_exists = self.module.triggers.iter().any(|t| {
                        t.name.eq_ignore_ascii_case("ON-INSERT")
                            && t.block_name
                                .as_deref()
                                .map(|b| b.eq_ignore_ascii_case(&b_upper))
                                .unwrap_or(false)
                    });

                    if on_ins_exists {
                        let on_ins = self.dispatch_event("ON-INSERT", None, Some(&b_name));
                        if !on_ins.is_success() {
                            self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                            return Err(RuntimeError::TriggerFailure("ON-INSERT failed".to_string()));
                        }
                    } else if let (Some(driver), Some(b_meta)) = (&self.database_driver, &b_info) {
                        if let Ok(ins_sql) = oracle_forms_db::SqlGenerator::generate_insert(
                            b_meta,
                            driver.backend(),
                            &current_values,
                        ) {
                            let ins_fut = driver.query(&ins_sql);
                            let _ = block_on_future(ins_fut);
                        }
                    }

                    let post_ins = self.dispatch_event("POST-INSERT", None, Some(&b_name));
                    if !post_ins.is_success() {
                        self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                        return Err(RuntimeError::TriggerFailure("POST-INSERT failed".to_string()));
                    }
                }
                // Case C: Updates
                else if is_update {
                    let pre_upd = self.dispatch_event("PRE-UPDATE", None, Some(&b_name));
                    if !pre_upd.is_success() {
                        self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                        return Err(RuntimeError::TriggerFailure("PRE-UPDATE failed".to_string()));
                    }

                    let current_values = self
                        .blocks
                        .get(&b_upper)
                        .map(|b| b.records[rec_idx].values.clone())
                        .unwrap_or(values_clone);

                    // Check ON-UPDATE trigger
                    let on_upd_exists = self.module.triggers.iter().any(|t| {
                        t.name.eq_ignore_ascii_case("ON-UPDATE")
                            && t.block_name
                                .as_deref()
                                .map(|b| b.eq_ignore_ascii_case(&b_upper))
                                .unwrap_or(false)
                    });

                    if on_upd_exists {
                        let on_upd = self.dispatch_event("ON-UPDATE", None, Some(&b_name));
                        if !on_upd.is_success() {
                            self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                            return Err(RuntimeError::TriggerFailure("ON-UPDATE failed".to_string()));
                        }
                    } else if let (Some(driver), Some(row_id), Some(b_meta)) =
                        (&self.database_driver, &row_id_opt, &b_info)
                    {
                        if let Ok(upd_sql) = oracle_forms_db::SqlGenerator::generate_update(
                            b_meta,
                            driver.backend(),
                            &current_values,
                            row_id,
                        ) {
                            let upd_fut = driver.query(&upd_sql);
                            let _ = block_on_future(upd_fut);
                        }
                    }

                    let post_upd = self.dispatch_event("POST-UPDATE", None, Some(&b_name));
                    if !post_upd.is_success() {
                        self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
                        return Err(RuntimeError::TriggerFailure("POST-UPDATE failed".to_string()));
                    }
                }
            }
        }

        // 4. POST-FORMS-COMMIT
        let post_commit_res = self.dispatch_event("POST-FORMS-COMMIT", None, None);
        if !post_commit_res.is_success() {
            self.trace_log.push("ROLLBACK_TO_SAVEPOINT".to_string());
            return Err(RuntimeError::TriggerFailure("POST-FORMS-COMMIT failed".to_string()));
        }

        // 5. Database COMMIT & buffer status finalization
        if let Some(driver) = &self.database_driver {
            let commit_stmt = oracle_forms_db::model::BoundSql::new("COMMIT");
            let commit_fut = driver.query(&commit_stmt);
            let _ = block_on_future(commit_fut);
        }

        for buf in self.blocks.values_mut() {
            // Remove deleted records
            buf.records.retain(|r| !r.is_marked_for_delete);
            // Transition inserted and changed records to Query
            for rec in &mut buf.records {
                if rec.status == FormsRecordStatus::Insert
                    || rec.status == FormsRecordStatus::Changed
                {
                    rec.status = FormsRecordStatus::Query;
                    rec.initial_values = rec.values.clone();
                    rec.validation_status = crate::record::ValidationStatus::Valid;
                    rec.item_validation.clear();
                    rec.is_locked = false;
                }
            }
            if buf.current_record_index >= buf.records.len() && !buf.records.is_empty() {
                buf.current_record_index = buf.records.len() - 1;
            }
        }

        self.update_system_status();
        Ok(())
    }

    /// Queries the block using the provided database driver and synchronizes master-detail relations.
    pub async fn execute_query(
        &mut self,
        block_name: &str,
        driver: Option<&dyn DatabaseDriver>,
    ) -> Result<QueryResult, RuntimeError> {
        let b_upper = block_name.to_ascii_uppercase();
        let block_info = self
            .module
            .blocks
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(block_name))
            .cloned()
            .ok_or_else(|| RuntimeError::BlockNotFound(block_name.to_string()))?;

        // 1. PRE-QUERY
        self.dispatch_event("PRE-QUERY", None, Some(block_name));

        // 2. Execute query
        let query_result = if let Some(drv) = driver {
            drv.query_block(&block_info, Some(50))
                .await
                .map_err(|e| RuntimeError::Database(e.to_string()))?
        } else {
            // Offline mock result
            QueryResult {
                block_name: block_name.to_string(),
                table_name: block_info
                    .query_data_source_name
                    .clone()
                    .unwrap_or_default(),
                columns: vec![],
                records: vec![],
                total_count: 0,
                execution_time_ms: 0,
                sql_statement: format!("SELECT * FROM {}", block_name),
            }
        };

        self.system.last_query = Some(query_result.sql_statement.clone());

        // 3. Populate block buffer
        if let Some(buf) = self.blocks.get_mut(&b_upper) {
            buf.populate_from_query_result(query_result.clone());

            // 4. POST-QUERY for each record
            for _ in 0..buf.records.len() {
                self.dispatch_event("POST-QUERY", None, Some(block_name));
            }
        }

        // 5. Synchronize detail relations
        self.synchronize_details(block_name, driver).await?;

        self.update_system_status();
        Ok(query_result)
    }

    /// Synchronizes detail blocks when master block's active row changes.
    pub async fn synchronize_details(
        &mut self,
        master_block_name: &str,
        driver: Option<&dyn DatabaseDriver>,
    ) -> Result<(), RuntimeError> {
        let b_upper = master_block_name.to_ascii_uppercase();
        let links: Vec<MasterDetailLink> = self
            .master_detail_links
            .iter()
            .filter(|l| l.master_block.eq_ignore_ascii_case(&b_upper) && l.auto_query)
            .cloned()
            .collect();

        if links.is_empty() {
            return Ok(());
        }

        let master_values = self
            .blocks
            .get(&b_upper)
            .and_then(|b| b.current_record())
            .map(|r| r.values.clone())
            .unwrap_or_default();

        for link in links {
            let detail_upper = link.detail_block.to_ascii_uppercase();
            let detail_where = link.evaluate_detail_where(&master_values);

            // Fetch detail block info and clone with dynamic WHERE clause
            if let Some(detail_block) = self
                .module
                .blocks
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(&link.detail_block))
                .cloned()
            {
                let mut customized_detail = detail_block;
                let mut dml = customized_detail.dml.unwrap_or_default();
                dml.where_clause = Some(detail_where);
                customized_detail.dml = Some(dml);

                if let Some(drv) = driver {
                    if let Ok(res) = drv.query_block(&customized_detail, Some(50)).await {
                        if let Some(buf) = self.blocks.get_mut(&detail_upper) {
                            buf.populate_from_query_result(res);
                        }
                    }
                }
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
