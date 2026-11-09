use crate::master_detail::MasterDetailLink;
use crate::record::{BlockBuffer, FormsRecordStatus};
use crate::system::{OperationalMode, SystemVariables};
use crate::trigger::{ExecutionHierarchy, ResolvedTrigger, TriggerResolver, TriggerResult};
use oracle_forms_core::model::ModuleIndex;
use oracle_forms_db::{DatabaseDriver, QueryResult};
use std::collections::HashMap;

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
}

pub type ValidationHook = Box<dyn Fn(&str, &str, &serde_json::Value) -> Result<(), String> + Send + Sync>;

/// Active in-memory Form runtime session.
pub struct FormSession {
    pub module: ModuleIndex,
    pub blocks: HashMap<String, BlockBuffer>,
    pub system: SystemVariables,
    pub master_detail_links: Vec<MasterDetailLink>,
    pub trace_log: Vec<String>,
    pub hierarchy: ExecutionHierarchy,
    pub validation_hook: Option<ValidationHook>,
}

impl FormSession {
    /// Creates a new FormSession from a parsed module definition.
    pub fn new(module: ModuleIndex) -> Self {
        let mut blocks = HashMap::new();
        let mut master_detail_links = Vec::new();

        for b in &module.blocks {
            let b_upper = b.name.to_ascii_uppercase();
            blocks.insert(b_upper.clone(), BlockBuffer::new(b.name.clone()));

            for rel in &b.relations {
                if let Some(link) = MasterDetailLink::from_relation(&b.name, rel) {
                    master_detail_links.push(link);
                }
            }
        }

        Self {
            module,
            blocks,
            system: SystemVariables::default(),
            master_detail_links,
            trace_log: Vec::new(),
            hierarchy: ExecutionHierarchy::Override,
            validation_hook: None,
        }
    }

    /// Sets custom validation hook for simulating trigger validation logic.
    pub fn set_validation_hook(&mut self, hook: ValidationHook) {
        self.validation_hook = Some(hook);
    }

    /// Emits and records a lifecycle or trigger event in the session trace log.
    fn dispatch_event(
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

            // If this is WHEN-VALIDATE-ITEM and a validation hook is provided, evaluate it
            if event_name.eq_ignore_ascii_case("WHEN-VALIDATE-ITEM") {
                if let (Some(b_name), Some(i_name)) = (block, item) {
                    if let Some(hook) = &self.validation_hook {
                        let b_upper = b_name.to_ascii_uppercase();
                        if let Some(buf) = self.blocks.get(&b_upper) {
                            if let Some(rec) = buf.current_record() {
                                let val = rec.get_value(i_name).unwrap_or(&serde_json::Value::Null);
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
            .and_then(|b| b.items.iter().find(|i| {
                !i.item_type.as_deref().map(|t| t.eq_ignore_ascii_case("Push Button")).unwrap_or(false)
            }))
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "ITEM1".to_string());

        let b_name = first_block.as_str();
        let i_name = first_item.as_str();

        // 1. PRE-FORM
        self.dispatch_event("PRE-FORM", None, None);

        // 2. PRE-BLOCK
        self.dispatch_event("PRE-BLOCK", None, Some(b_name));

        // 3. PRE-RECORD
        self.dispatch_event("PRE-RECORD", None, Some(b_name));

        // 4. PRE-TEXT-ITEM
        self.dispatch_event("PRE-TEXT-ITEM", Some(i_name), Some(b_name));

        // 5. WHEN-NEW-FORM-INSTANCE
        self.dispatch_event("WHEN-NEW-FORM-INSTANCE", None, None);

        // 6. WHEN-NEW-BLOCK-INSTANCE
        self.dispatch_event("WHEN-NEW-BLOCK-INSTANCE", None, Some(b_name));

        // 7. WHEN-NEW-RECORD-INSTANCE
        self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(b_name));

        // 8. WHEN-NEW-ITEM-INSTANCE
        self.dispatch_event("WHEN-NEW-ITEM-INSTANCE", Some(i_name), Some(b_name));

        // Set cursor focus
        self.system.cursor_block = Some(b_name.to_string());
        self.system.cursor_item = Some(format!("{}.{}", b_name, i_name));
        self.system.cursor_record = 1;
        self.system.mode = OperationalMode::Normal;

        Ok(())
    }

    /// Moves cursor focus to target item, cascading validation and navigation triggers.
    pub fn go_item(&mut self, target: &str) -> Result<(), RuntimeError> {
        let (target_block, target_item) = if let Some((b, i)) = target.split_once('.') {
            (b.to_string(), i.to_string())
        } else {
            let active_b = self
                .system
                .cursor_block
                .clone()
                .unwrap_or_else(|| "BLOCK1".to_string());
            (active_b, target.to_string())
        };

        let curr_item = self.system.cursor_item.clone();
        let (curr_block_opt, curr_item_opt) = curr_item
            .as_deref()
            .and_then(|ci| ci.split_once('.'))
            .map(|(b, i)| (Some(b.to_string()), Some(i.to_string())))
            .unwrap_or((None, None));

        // 1. Validation Cascade on Source Field (if leaving item and item is dirty)
        if let (Some(cb), Some(ci)) = (&curr_block_opt, &curr_item_opt) {
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
                let res = self.dispatch_event("WHEN-VALIDATE-ITEM", Some(ci), Some(cb));
                if let TriggerResult::Failure(msg) = res {
                    return Err(RuntimeError::ValidationFailed {
                        item: format!("{}.{}", cb, ci),
                        message: msg,
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

    /// Mutates an item's value in the current record, marking it dirty and transitioning record status.
    pub fn set_item_value(&mut self, item_path: &str, value: serde_json::Value) -> Result<(), RuntimeError> {
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
        let buf = self
            .blocks
            .get_mut(&b_upper)
            .ok_or_else(|| RuntimeError::BlockNotFound(b_name.clone()))?;

        if let Some(rec) = buf.current_record_mut() {
            rec.set_value(&i_name, value);
        } else {
            buf.create_record();
            if let Some(rec) = buf.current_record_mut() {
                rec.set_value(&i_name, value);
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
                table_name: block_info.query_data_source_name.clone().unwrap_or_default(),
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

    /// Moves cursor to next record in block.
    pub async fn next_record(
        &mut self,
        block_name: &str,
        driver: Option<&dyn DatabaseDriver>,
    ) -> Result<(), RuntimeError> {
        let b_upper = block_name.to_ascii_uppercase();
        let (can_advance, new_idx) = {
            let buf = self
                .blocks
                .get(&b_upper)
                .ok_or_else(|| RuntimeError::BlockNotFound(block_name.to_string()))?;
            if buf.current_record_index + 1 < buf.records.len() {
                (true, buf.current_record_index + 1)
            } else {
                (false, buf.current_record_index)
            }
        };

        if can_advance {
            self.dispatch_event("POST-RECORD", None, Some(block_name));
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                buf.current_record_index = new_idx;
            }
            self.system.cursor_record = new_idx + 1;
            self.dispatch_event("PRE-RECORD", None, Some(block_name));
            self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(block_name));

            self.synchronize_details(block_name, driver).await?;
            self.update_system_status();
        }

        Ok(())
    }

    /// Moves cursor to previous record in block.
    pub async fn previous_record(
        &mut self,
        block_name: &str,
        driver: Option<&dyn DatabaseDriver>,
    ) -> Result<(), RuntimeError> {
        let b_upper = block_name.to_ascii_uppercase();
        let (can_retreat, new_idx) = {
            let buf = self
                .blocks
                .get(&b_upper)
                .ok_or_else(|| RuntimeError::BlockNotFound(block_name.to_string()))?;
            if buf.current_record_index > 0 {
                (true, buf.current_record_index - 1)
            } else {
                (false, 0)
            }
        };

        if can_retreat {
            self.dispatch_event("POST-RECORD", None, Some(block_name));
            if let Some(buf) = self.blocks.get_mut(&b_upper) {
                buf.current_record_index = new_idx;
            }
            self.system.cursor_record = new_idx + 1;
            self.dispatch_event("PRE-RECORD", None, Some(block_name));
            self.dispatch_event("WHEN-NEW-RECORD-INSTANCE", None, Some(block_name));

            self.synchronize_details(block_name, driver).await?;
            self.update_system_status();
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

        let is_changed = self.blocks.values().any(|b| b.status() == FormsRecordStatus::Changed);
        let is_insert = self.blocks.values().any(|b| b.status() == FormsRecordStatus::Insert);
        let is_query = self.blocks.values().any(|b| b.status() == FormsRecordStatus::Query);

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
