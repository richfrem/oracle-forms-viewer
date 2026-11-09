use oracle_forms_core::model::{TriggerInfo, TriggerLevel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionHierarchy {
    /// Lowest defined level executes; higher-level triggers are skipped.
    #[default]
    Override,
    /// Higher-level trigger executes before lower-level trigger.
    Before,
    /// Higher-level trigger executes after lower-level trigger.
    After,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerResult {
    Success,
    /// Handled Forms exception: RAISE FORM_TRIGGER_FAILURE
    Failure(String),
    /// Unhandled PL/SQL exception
    UnhandledException(String),
}

impl TriggerResult {
    pub fn is_success(&self) -> bool {
        matches!(self, TriggerResult::Success)
    }
}

/// Information about a resolved trigger execution candidate.
#[derive(Debug, Clone)]
pub struct ResolvedTrigger {
    pub name: String,
    pub level: TriggerLevel,
    pub block_name: Option<String>,
    pub item_name: Option<String>,
    pub hierarchy: ExecutionHierarchy,
    pub trigger_info: Option<TriggerInfo>,
}

pub struct TriggerResolver;

impl TriggerResolver {
    /// Resolves active triggers for an event, ordering candidates based on ExecutionHierarchy (Override, Before, After).
    pub fn resolve(
        event_name: &str,
        item: Option<&str>,
        block: Option<&str>,
        module_triggers: &[TriggerInfo],
        hierarchy: ExecutionHierarchy,
    ) -> Vec<ResolvedTrigger> {
        let event_upper = event_name.to_ascii_uppercase();
        let mut candidates = Vec::new();

        // 1. Check Item Level
        if let (Some(b_name), Some(i_name)) = (block, item) {
            let b_upper = b_name.to_ascii_uppercase();
            let i_upper = i_name.to_ascii_uppercase();
            for trig in module_triggers {
                if trig.name.eq_ignore_ascii_case(&event_upper)
                    && trig.level == TriggerLevel::Item
                    && trig.block_name.as_deref().map(|s| s.eq_ignore_ascii_case(&b_upper)).unwrap_or(false)
                    && trig.item_name.as_deref().map(|s| s.eq_ignore_ascii_case(&i_upper)).unwrap_or(false)
                {
                    candidates.push(ResolvedTrigger {
                        name: trig.name.clone(),
                        level: TriggerLevel::Item,
                        block_name: Some(b_name.to_string()),
                        item_name: Some(i_name.to_string()),
                        hierarchy,
                        trigger_info: Some(trig.clone()),
                    });
                }
            }
        }

        // 2. Check Block Level
        if let Some(b_name) = block {
            let b_upper = b_name.to_ascii_uppercase();
            for trig in module_triggers {
                if trig.name.eq_ignore_ascii_case(&event_upper)
                    && trig.level == TriggerLevel::Block
                    && trig.block_name.as_deref().map(|s| s.eq_ignore_ascii_case(&b_upper)).unwrap_or(false)
                {
                    candidates.push(ResolvedTrigger {
                        name: trig.name.clone(),
                        level: TriggerLevel::Block,
                        block_name: Some(b_name.to_string()),
                        item_name: None,
                        hierarchy,
                        trigger_info: Some(trig.clone()),
                    });
                }
            }
        }

        // 3. Check Form Level
        for trig in module_triggers {
            if trig.name.eq_ignore_ascii_case(&event_upper) && trig.level == TriggerLevel::Form {
                candidates.push(ResolvedTrigger {
                    name: trig.name.clone(),
                    level: TriggerLevel::Form,
                    block_name: None,
                    item_name: None,
                    hierarchy,
                    trigger_info: Some(trig.clone()),
                });
            }
        }

        if candidates.is_empty() {
            // Synthesize default entry so lifecycle events are logged in the trace
            let lvl = if item.is_some() {
                TriggerLevel::Item
            } else if block.is_some() {
                TriggerLevel::Block
            } else {
                TriggerLevel::Form
            };
            return vec![ResolvedTrigger {
                name: event_upper,
                level: lvl,
                block_name: block.map(|s| s.to_string()),
                item_name: item.map(|s| s.to_string()),
                hierarchy,
                trigger_info: None,
            }];
        }

        // Apply Execution Hierarchy ordering
        match hierarchy {
            ExecutionHierarchy::Override => {
                // Only execute the lowest defined level (Item > Block > Form)
                if let Some(first) = candidates.into_iter().next() {
                    vec![first]
                } else {
                    vec![]
                }
            }
            ExecutionHierarchy::Before => {
                // Higher-level executes before lower-level: Form -> Block -> Item
                candidates.sort_by_key(|t| match t.level {
                    TriggerLevel::Form => 0,
                    TriggerLevel::Block => 1,
                    TriggerLevel::Item => 2,
                    TriggerLevel::Menu => 3,
                });
                candidates
            }
            ExecutionHierarchy::After => {
                // Higher-level executes after lower-level: Item -> Block -> Form
                candidates.sort_by_key(|t| match t.level {
                    TriggerLevel::Item => 0,
                    TriggerLevel::Block => 1,
                    TriggerLevel::Form => 2,
                    TriggerLevel::Menu => 3,
                });
                candidates
            }
        }
    }
}
