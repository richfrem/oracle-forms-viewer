use crate::record::FormsRecordStatus;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperationalMode {
    #[default]
    Normal,
    EnterQuery,
    Query,
}

impl OperationalMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            OperationalMode::Normal => "NORMAL",
            OperationalMode::EnterQuery => "ENTER-QUERY",
            OperationalMode::Query => "QUERY",
        }
    }
}

/// Oracle Forms runtime system variables accessible via :SYSTEM.*
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemVariables {
    /// Form and item holding active cursor focus (BLOCK.ITEM)
    pub cursor_item: Option<String>,
    /// Name of active block
    pub cursor_block: Option<String>,
    /// 1-based index of active record
    pub cursor_record: usize,
    /// Item that triggered the executing event
    pub trigger_item: Option<String>,
    /// Block containing the trigger item
    pub trigger_block: Option<String>,
    /// Status of active record: NEW, INSERT, QUERY, CHANGED
    pub record_status: FormsRecordStatus,
    /// Aggregate status of active block: NEW, QUERY, CHANGED
    pub block_status: FormsRecordStatus,
    /// Aggregate status of the form: NEW, QUERY, CHANGED
    pub form_status: FormsRecordStatus,
    /// Operational mode: NORMAL, ENTER-QUERY, QUERY
    pub mode: OperationalMode,
    /// Raw SQL string executed by the last block query
    pub last_query: Option<String>,
    /// Minimum severity level of messages displayed to the user
    pub message_level: u8,
    /// Suppresses runtime working indicators
    pub suppress_working: bool,
}

impl Default for SystemVariables {
    fn default() -> Self {
        Self {
            cursor_item: None,
            cursor_block: None,
            cursor_record: 1,
            trigger_item: None,
            trigger_block: None,
            record_status: FormsRecordStatus::New,
            block_status: FormsRecordStatus::New,
            form_status: FormsRecordStatus::New,
            mode: OperationalMode::Normal,
            last_query: None,
            message_level: 0,
            suppress_working: false,
        }
    }
}

impl SystemVariables {
    /// Resolves system variable by canonical name (e.g. "CURSOR_ITEM" or ":SYSTEM.CURSOR_ITEM")
    pub fn get_var(&self, name: &str) -> Option<String> {
        let clean = name
            .trim()
            .trim_start_matches(':')
            .strip_prefix("SYSTEM.")
            .unwrap_or(name)
            .to_ascii_uppercase();

        match clean.as_str() {
            "CURSOR_ITEM" => self.cursor_item.clone(),
            "CURSOR_BLOCK" => self.cursor_block.clone(),
            "CURSOR_RECORD" => Some(self.cursor_record.to_string()),
            "TRIGGER_ITEM" => self.trigger_item.clone(),
            "TRIGGER_BLOCK" => self.trigger_block.clone(),
            "RECORD_STATUS" => Some(self.record_status.as_str().to_string()),
            "BLOCK_STATUS" => Some(self.block_status.as_str().to_string()),
            "FORM_STATUS" => Some(self.form_status.as_str().to_string()),
            "MODE" => Some(self.mode.as_str().to_string()),
            "LAST_QUERY" => self.last_query.clone(),
            "MESSAGE_LEVEL" => Some(self.message_level.to_string()),
            "SUPPRESS_WORKING" => Some(if self.suppress_working { "TRUE" } else { "FALSE" }.to_string()),
            _ => None,
        }
    }

    /// Mutates system variable by canonical name.
    pub fn set_var(&mut self, name: &str, val: &str) {
        let clean = name
            .trim()
            .trim_start_matches(':')
            .strip_prefix("SYSTEM.")
            .unwrap_or(name)
            .to_ascii_uppercase();

        match clean.as_str() {
            "CURSOR_ITEM" => self.cursor_item = Some(val.to_string()),
            "CURSOR_BLOCK" => self.cursor_block = Some(val.to_string()),
            "CURSOR_RECORD" => {
                if let Ok(n) = val.parse::<usize>() {
                    self.cursor_record = n;
                }
            }
            "TRIGGER_ITEM" => self.trigger_item = Some(val.to_string()),
            "TRIGGER_BLOCK" => self.trigger_block = Some(val.to_string()),
            "MODE" => {
                self.mode = if val.eq_ignore_ascii_case("ENTER-QUERY") {
                    OperationalMode::EnterQuery
                } else if val.eq_ignore_ascii_case("QUERY") {
                    OperationalMode::Query
                } else {
                    OperationalMode::Normal
                };
            }
            "LAST_QUERY" => self.last_query = Some(val.to_string()),
            "MESSAGE_LEVEL" => {
                if let Ok(n) = val.parse::<u8>() {
                    self.message_level = n;
                }
            }
            "SUPPRESS_WORKING" => {
                self.suppress_working = val.eq_ignore_ascii_case("TRUE");
            }
            _ => {}
        }
    }
}
