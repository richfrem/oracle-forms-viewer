use oracle_forms_db::QueryResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Oracle Forms Record Status as exposed via :SYSTEM.RECORD_STATUS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FormsRecordStatus {
    /// A blank record created by navigation or CREATE_RECORD. Not saved during commit.
    #[default]
    New,
    /// A new record with user modifications; targeted for SQL INSERT during commit.
    Insert,
    /// A record fetched from the database that has not been modified.
    Query,
    /// A queried record modified by the user or code; targeted for SQL UPDATE during commit.
    Changed,
}

impl FormsRecordStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            FormsRecordStatus::New => "NEW",
            FormsRecordStatus::Insert => "INSERT",
            FormsRecordStatus::Query => "QUERY",
            FormsRecordStatus::Changed => "CHANGED",
        }
    }
}

/// Validation lifecycle state for items and records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationStatus {
    /// Pristine untouched state.
    #[default]
    New,
    /// Dirty; requires validation before leaving item/record.
    Changed,
    /// Validation completed and verified.
    Valid,
}

/// A single in-memory record within a block's multi-record buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormRecord {
    pub status: FormsRecordStatus,
    pub validation_status: ValidationStatus,
    pub is_marked_for_delete: bool,
    pub values: HashMap<String, serde_json::Value>,
    pub initial_values: HashMap<String, serde_json::Value>,
    pub item_validation: HashMap<String, ValidationStatus>,
    pub row_id: Option<String>,
}

impl Default for FormRecord {
    fn default() -> Self {
        Self::new()
    }
}

impl FormRecord {
    pub fn new() -> Self {
        Self {
            status: FormsRecordStatus::New,
            validation_status: ValidationStatus::New,
            is_marked_for_delete: false,
            values: HashMap::new(),
            initial_values: HashMap::new(),
            item_validation: HashMap::new(),
            row_id: None,
        }
    }

    pub fn with_query_values(
        values: HashMap<String, serde_json::Value>,
        row_id: Option<String>,
    ) -> Self {
        Self {
            status: FormsRecordStatus::Query,
            validation_status: ValidationStatus::Valid,
            is_marked_for_delete: false,
            initial_values: values.clone(),
            values,
            item_validation: HashMap::new(),
            row_id,
        }
    }

    pub fn get_value(&self, item_name: &str) -> Option<&serde_json::Value> {
        let key_upper = item_name.to_ascii_uppercase();
        self.values.get(&key_upper)
    }

    pub fn set_value(&mut self, item_name: &str, val: serde_json::Value) {
        let key_upper = item_name.to_ascii_uppercase();
        self.values.insert(key_upper.clone(), val);
        self.item_validation.insert(key_upper, ValidationStatus::Changed);
        self.validation_status = ValidationStatus::Changed;

        // Transition record status
        match self.status {
            FormsRecordStatus::New => self.status = FormsRecordStatus::Insert,
            FormsRecordStatus::Query => self.status = FormsRecordStatus::Changed,
            FormsRecordStatus::Insert | FormsRecordStatus::Changed => {}
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.status == FormsRecordStatus::Insert || self.status == FormsRecordStatus::Changed
    }

    pub fn is_item_dirty(&self, item_name: &str) -> bool {
        let key_upper = item_name.to_ascii_uppercase();
        self.item_validation
            .get(&key_upper)
            .map(|v| *v == ValidationStatus::Changed)
            .unwrap_or(false)
    }

    pub fn mark_item_valid(&mut self, item_name: &str) {
        let key_upper = item_name.to_ascii_uppercase();
        self.item_validation.insert(key_upper, ValidationStatus::Valid);
    }

    pub fn mark_record_valid(&mut self) {
        self.validation_status = ValidationStatus::Valid;
        for v in self.item_validation.values_mut() {
            *v = ValidationStatus::Valid;
        }
    }
}

/// The multi-record buffer for an active database or control block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBuffer {
    pub block_name: String,
    pub records: Vec<FormRecord>,
    pub current_record_index: usize, // 0-based internal index
    pub cursor_fetched_all: bool,
}

impl BlockBuffer {
    pub fn new(block_name: impl Into<String>) -> Self {
        let mut buffer = Self {
            block_name: block_name.into(),
            records: Vec::new(),
            current_record_index: 0,
            cursor_fetched_all: true,
        };
        // Always initialize with one initial blank NEW record as Forms does
        buffer.records.push(FormRecord::new());
        buffer
    }

    pub fn current_record(&self) -> Option<&FormRecord> {
        self.records.get(self.current_record_index)
    }

    pub fn current_record_mut(&mut self) -> Option<&mut FormRecord> {
        self.records.get_mut(self.current_record_index)
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    pub fn create_record(&mut self) -> usize {
        let new_rec = FormRecord::new();
        if self.records.is_empty() {
            self.records.push(new_rec);
            0
        } else {
            self.current_record_index += 1;
            self.records.insert(self.current_record_index, new_rec);
            self.current_record_index
        }
    }

    pub fn delete_record(&mut self) -> bool {
        if let Some(rec) = self.current_record_mut() {
            rec.is_marked_for_delete = true;
            true
        } else {
            false
        }
    }

    pub fn clear_record(&mut self) {
        if self.records.is_empty() {
            return;
        }
        let status = self.records[self.current_record_index].status;
        if status == FormsRecordStatus::New || status == FormsRecordStatus::Insert {
            self.records.remove(self.current_record_index);
            if self.records.is_empty() {
                self.records.push(FormRecord::new());
                self.current_record_index = 0;
            } else if self.current_record_index >= self.records.len() {
                self.current_record_index = self.records.len() - 1;
            }
        } else {
            // Revert to initial_values
            let rec = &mut self.records[self.current_record_index];
            rec.values = rec.initial_values.clone();
            rec.status = FormsRecordStatus::Query;
            rec.validation_status = ValidationStatus::Valid;
            rec.item_validation.clear();
        }
    }

    pub fn clear_block(&mut self) {
        self.records.clear();
        self.records.push(FormRecord::new());
        self.current_record_index = 0;
        self.cursor_fetched_all = true;
    }

    pub fn status(&self) -> FormsRecordStatus {
        if self.records.iter().any(|r| r.status == FormsRecordStatus::Changed) {
            FormsRecordStatus::Changed
        } else if self.records.iter().any(|r| r.status == FormsRecordStatus::Insert) {
            FormsRecordStatus::Insert
        } else if self.records.iter().any(|r| r.status == FormsRecordStatus::Query) {
            FormsRecordStatus::Query
        } else {
            FormsRecordStatus::New
        }
    }

    pub fn populate_from_query_result(&mut self, query_res: QueryResult) {
        self.records.clear();
        for q_rec in query_res.records {
            let record = FormRecord::with_query_values(q_rec.values, q_rec.row_id);
            self.records.push(record);
        }
        if self.records.is_empty() {
            self.records.push(FormRecord::new());
        }
        self.current_record_index = 0;
        self.cursor_fetched_all = true;
    }
}
