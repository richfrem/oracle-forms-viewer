use crate::session::FormSession;
use crate::value::OracleValue;
use serde::{Deserialize, Serialize};

pub const ALERT_BUTTON1: i64 = 81;
pub const ALERT_BUTTON2: i64 = 82;
pub const ALERT_BUTTON3: i64 = 83;

pub const PROPERTY_TRUE: i64 = 4;
pub const PROPERTY_FALSE: i64 = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BuiltinResult {
    /// Return a value from function call
    Value(OracleValue),
    /// Procedure completed successfully without return value
    Void,
    /// Resumable interaction request (e.g. SHOW_ALERT in interactive mode)
    Suspend(BuiltinSuspension),
    /// Handled error / failure
    Failure(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BuiltinSuspension {
    ShowAlert {
        alert_name: String,
        message: String,
        button1: String,
        button2: Option<String>,
        button3: Option<String>,
        default_button: u8,
    },
}

pub struct BuiltinRegistry;

impl BuiltinRegistry {
    /// Dispatches a Forms built-in procedure or function by name.
    pub fn dispatch(
        name: &str,
        args: &[OracleValue],
        session: &mut FormSession,
        interactive: bool,
    ) -> BuiltinResult {
        let name_upper = name.to_ascii_uppercase();
        session.trace_log.push(format!("BUILTIN: {}", name_upper));

        match name_upper.as_str() {
            // Navigation built-ins
            "GO_ITEM" => {
                if let Some(arg) = args.first().and_then(|v| v.as_str()) {
                    if let Err(e) = session.go_item(arg) {
                        return BuiltinResult::Failure(e.to_string());
                    }
                }
                BuiltinResult::Void
            }
            "GO_BLOCK" => {
                if let Some(arg) = args.first().and_then(|v| v.as_str()) {
                    if let Err(e) = session.go_block(arg) {
                        return BuiltinResult::Failure(e.to_string());
                    }
                }
                BuiltinResult::Void
            }
            "NEXT_ITEM" => {
                if let Err(e) = session.next_item() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }
            "PREVIOUS_ITEM" | "PREV_ITEM" => {
                if let Err(e) = session.previous_item() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }
            "NEXT_RECORD" => {
                if let Err(e) = session.next_record() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }
            "PREVIOUS_RECORD" | "PREV_RECORD" => {
                if let Err(e) = session.previous_record() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }
            "FIRST_RECORD" => {
                if let Err(e) = session.first_record() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }
            "LAST_RECORD" => {
                if let Err(e) = session.last_record() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }

            // Query & record lifecycle
            "EXECUTE_QUERY" => {
                if let Some(block) = session.active_block() {
                    let block_name = block.to_string();
                    if let Err(e) = session.execute_query_on_block(&block_name) {
                        return BuiltinResult::Failure(e.to_string());
                    }
                }
                BuiltinResult::Void
            }
            "ENTER_QUERY" => {
                session.system.set_var("MODE", "ENTER-QUERY");
                BuiltinResult::Void
            }
            "CREATE_RECORD" => {
                if let Err(e) = session.create_record() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }
            "DELETE_RECORD" => {
                if let Err(e) = session.delete_record() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }
            "CLEAR_BLOCK" => {
                if let Some(block) = session.active_block() {
                    let b = block.to_string();
                    session.clear_block(&b);
                }
                BuiltinResult::Void
            }
            "CLEAR_FORM" => {
                session.clear_form();
                BuiltinResult::Void
            }
            "COMMIT" | "COMMIT_FORM" => {
                if let Err(e) = session.commit() {
                    return BuiltinResult::Failure(e.to_string());
                }
                BuiltinResult::Void
            }
            "DO_KEY" => {
                if let Some(arg) = args.first().and_then(|v| v.as_str()) {
                    session.trace_log.push(format!("DO_KEY: {}", arg));
                    if arg.eq_ignore_ascii_case("COMMIT_FORM") {
                        if let Err(e) = session.commit() {
                            return BuiltinResult::Failure(e.to_string());
                        }
                    }
                }
                BuiltinResult::Void
            }

            // Messages & Alerts
            "MESSAGE" => {
                let msg = args
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                session.messages.push(msg.clone());
                session.trace_log.push(format!("MESSAGE: {}", msg));
                BuiltinResult::Void
            }
            "SHOW_ALERT" => {
                let alert_name = args
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("ALERT")
                    .to_ascii_uppercase();

                let (message, b1, b2, b3, def) = session
                    .alerts
                    .get(&alert_name)
                    .map(|a| {
                        (
                            a.message.clone(),
                            a.button1.clone(),
                            a.button2.clone(),
                            a.button3.clone(),
                            a.default_button,
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            format!("Alert: {}", alert_name),
                            "OK".to_string(),
                            Some("Cancel".to_string()),
                            None,
                            1,
                        )
                    });

                if interactive {
                    BuiltinResult::Suspend(BuiltinSuspension::ShowAlert {
                        alert_name,
                        message,
                        button1: b1,
                        button2: b2,
                        button3: b3,
                        default_button: def,
                    })
                } else {
                    // Non-interactive fallback: auto-select default button (ALERT_BUTTON1 = 81)
                    BuiltinResult::Value(OracleValue::Number(ALERT_BUTTON1 as f64))
                }
            }

            // Dynamic access
            "NAME_IN" => {
                if let Some(var_name) = args.first().and_then(|v| v.as_str()) {
                    let val = session.get_bind_value(var_name);
                    BuiltinResult::Value(val)
                } else {
                    BuiltinResult::Value(OracleValue::Null)
                }
            }
            "COPY" => {
                if args.len() >= 2 {
                    let val = &args[0];
                    if let Some(target) = args[1].as_str() {
                        session.set_bind_value(target, val.clone());
                    }
                }
                BuiltinResult::Void
            }
            "DEFAULT_VALUE" => {
                if args.len() >= 2 {
                    let val = &args[0];
                    if let Some(target) = args[1].as_str() {
                        if session.get_bind_value(target).is_null() {
                            session.set_bind_value(target, val.clone());
                        }
                    }
                }
                BuiltinResult::Void
            }
            "ERASE" => {
                if let Some(var_name) = args.first().and_then(|v| v.as_str()) {
                    session.erase_global(var_name);
                }
                BuiltinResult::Void
            }

            // Properties
            "SET_ITEM_PROPERTY" => {
                if args.len() >= 3 {
                    if let (Some(item), Some(prop)) = (args[0].as_str(), args[1].as_str()) {
                        session.set_item_property(item, prop, args[2].clone());
                    }
                }
                BuiltinResult::Void
            }
            "GET_ITEM_PROPERTY" => {
                if args.len() >= 2 {
                    if let (Some(item), Some(prop)) = (args[0].as_str(), args[1].as_str()) {
                        let val = session.get_item_property(item, prop);
                        return BuiltinResult::Value(val);
                    }
                }
                BuiltinResult::Value(OracleValue::Null)
            }

            // Utility
            "ID_NULL" => {
                let is_null = args.first().map(|v| v.is_null()).unwrap_or(true);
                BuiltinResult::Value(OracleValue::Boolean(Some(is_null)))
            }
            "SYNCHRONIZE" => {
                session.trace_log.push("SYNCHRONIZE".to_string());
                BuiltinResult::Void
            }

            _ => {
                // Tier 3 unsupported builtin logging
                session.trace_log.push(format!("UNSUPPORTED_BUILTIN: {}", name_upper));
                BuiltinResult::Void
            }
        }
    }
}
