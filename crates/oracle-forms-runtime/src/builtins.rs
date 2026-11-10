//! Oracle Forms built-in subprograms.
//!
//! Semantics follow Forms: a built-in that fails (e.g. `GO_ITEM` to a missing item) does
//! **not** raise an exception. It shows a message, sets `FORM_FAILURE` to TRUE, and the
//! trigger continues. Code checks `FORM_SUCCESS` / `FORM_FAILURE` or raises
//! `FORM_TRIGGER_FAILURE` itself.
//!
//! Built-ins this runtime cannot provide (OS access, OLE, file I/O, reports, multi-form
//! navigation) are reported as unsupported instead of silently doing nothing.

use crate::interpreter::{InteractionRequest, InteractionResponse};
use crate::session::FormSession;
use crate::value::OracleValue;

pub const ALERT_BUTTON1: i64 = 81;
pub const ALERT_BUTTON2: i64 = 82;
pub const ALERT_BUTTON3: i64 = 83;

pub const PROPERTY_TRUE: i64 = 4;
pub const PROPERTY_FALSE: i64 = 5;

/// Named Forms constants that appear as bare identifiers in built-in calls
/// (`SET_ITEM_PROPERTY('B.I', ENABLED, PROPERTY_FALSE)`). They evaluate to their own name.
pub const FORMS_CONSTANTS: &[&str] = &[
    "ENABLED", "VISIBLE", "DISPLAYED", "NAVIGABLE", "UPDATEABLE", "UPDATE_ALLOWED", "INSERT_ALLOWED",
    "QUERYABLE", "QUERY_ALLOWED", "DELETE_ALLOWED", "REQUIRED", "LABEL", "PROMPT_TEXT", "HINT_TEXT",
    "TOOLTIP_TEXT", "BACKGROUND_COLOR", "FOREGROUND_COLOR", "VISUAL_ATTRIBUTE", "FONT_NAME", "FONT_SIZE",
    "FONT_WEIGHT", "WIDTH", "HEIGHT", "X_POS", "Y_POS", "POSITION", "ITEM_TYPE", "DATATYPE", "MAX_LENGTH",
    "FORMAT_MASK", "DEFAULT_WHERE", "ORDER_BY", "CURRENT_RECORD", "QUERY_DATA_SOURCE_NAME",
    "DML_DATA_TARGET_NAME", "WINDOW_STATE", "TITLE", "MAXIMIZE", "MINIMIZE", "NORMAL", "TOPMOST_TAB_PAGE",
    "LOV_NAME", "VALIDATE_FROM_LIST", "AUTO_SKIP", "CONCEAL_DATA", "CASE_RESTRICTION",
    "NO_VALIDATE", "DO_COMMIT", "NO_COMMIT", "FULL_ROLLBACK", "ASK_COMMIT", "NO_SCREEN",
    "PRIMARY_KEY", "LOCKING_MODE", "ITEM_IS_VALID", "STATUS", "RECORD_STATUS", "CURRENT_FORM",
    "USERNAME", "PASSWORD", "CONNECT_STRING", "OPERATING_SYSTEM", "USER_INTERFACE", "CALLING_FORM",
    "NO_ACKNOWLEDGE", "ACKNOWLEDGE", "TEXT_PARAMETER", "DATA_PARAMETER", "ALERT_MESSAGE_TEXT",
    "BUTTON1", "BUTTON2", "BUTTON3", "ALL_RECORDS", "FIRST_RECORD", "NEXT_RECORD", "LAST_RECORD",
    "HIDE", "NO_HIDE", "DO_REPLACE", "NO_REPLACE", "FORMS_BUILTIN", "PROPERTY_ON", "PROPERTY_OFF",
];

pub fn is_forms_constant(name: &str) -> bool {
    FORMS_CONSTANTS.iter().any(|c| c.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinResult {
    /// Function result.
    Value(OracleValue),
    /// Procedure completed.
    Void,
    /// Built-in failed: message shown, `FORM_FAILURE` set, execution continues.
    Failure(String),
    /// Built-in exists in Forms but this runtime cannot run it.
    Unsupported(String),
}

/// Built-ins that exist in Forms but are deliberately not provided by this runtime.
const UNSUPPORTED: &[(&str, &str)] = &[
    ("HOST", "runs an operating-system command"),
    ("RUN_PRODUCT", "launches Oracle Reports/Graphics"),
    ("RUN_REPORT_OBJECT", "runs an Oracle Report"),
    ("WEB.SHOW_DOCUMENT", "opens a browser window"),
    ("CALL_FORM", "multi-form navigation is not supported yet"),
    ("OPEN_FORM", "multi-form navigation is not supported yet"),
    ("NEW_FORM", "multi-form navigation is not supported yet"),
    ("SHOW_LOV", "LOV dialogs are not supported by the runtime yet"),
    ("LIST_VALUES", "LOV dialogs are not supported by the runtime yet"),
    ("CREATE_TIMER", "timers are not supported yet"),
    ("DELETE_TIMER", "timers are not supported yet"),
    ("EXEC_SQL", "dynamic SQL via EXEC_SQL is not supported"),
    ("FORMS_DDL", "dynamic SQL via FORMS_DDL is not supported"),
];
const UNSUPPORTED_PREFIXES: &[(&str, &str)] = &[
    ("OLE2.", "OLE automation"),
    ("TEXT_IO.", "server file I/O"),
    ("CLIENT_TEXT_IO.", "client file I/O"),
    ("WEBUTIL", "WebUtil client integration"),
    ("CLIENT_", "WebUtil client integration"),
    ("D2KWUTIL", "Windows utilities"),
    ("DDE.", "Windows DDE"),
];

fn arg_text(args: &[OracleValue], idx: usize) -> Option<String> {
    args.get(idx).and_then(|v| v.as_text())
}

/// Returns `Some(result)` if `name` is a Forms built-in, `None` otherwise (so the caller
/// can try user program units, standard SQL functions, or the database).
pub fn dispatch(name: &str, args: &[OracleValue], session: &mut FormSession) -> Option<BuiltinResult> {
    let name_upper = name.to_ascii_uppercase();

    if let Some((_, why)) = UNSUPPORTED.iter().find(|(n, _)| *n == name_upper) {
        return Some(BuiltinResult::Unsupported(format!("{} ({})", name_upper, why)));
    }
    if let Some((_, why)) = UNSUPPORTED_PREFIXES.iter().find(|(p, _)| name_upper.starts_with(p)) {
        return Some(BuiltinResult::Unsupported(format!("{} ({})", name_upper, why)));
    }

    let nav = |r: Result<(), crate::session::RuntimeError>| match r {
        Ok(()) => BuiltinResult::Void,
        Err(e) => BuiltinResult::Failure(e.to_string()),
    };

    let result = match name_upper.as_str() {
        // Navigation
        "GO_ITEM" => match arg_text(args, 0) {
            Some(item) => nav(session.go_item(&item)),
            None => BuiltinResult::Failure("GO_ITEM requires an item name".into()),
        },
        "GO_BLOCK" => match arg_text(args, 0) {
            Some(block) => nav(session.go_block(&block)),
            None => BuiltinResult::Failure("GO_BLOCK requires a block name".into()),
        },
        "GO_RECORD" => match args.first().and_then(|v| v.as_i64()) {
            Some(n) if n >= 1 => nav(session.go_record(n as usize)),
            _ => BuiltinResult::Failure("GO_RECORD requires a record number >= 1".into()),
        },
        "NEXT_ITEM" => nav(session.next_item()),
        "PREVIOUS_ITEM" => nav(session.previous_item()),
        "NEXT_RECORD" => nav(session.next_record()),
        "PREVIOUS_RECORD" => nav(session.previous_record()),
        "FIRST_RECORD" => nav(session.first_record()),
        "LAST_RECORD" => nav(session.last_record()),

        // Query & record lifecycle
        "EXECUTE_QUERY" => match session.active_block().map(str::to_string) {
            Some(block) => nav(session.execute_query_on_block(&block)),
            None => BuiltinResult::Failure("EXECUTE_QUERY: no current block".into()),
        },
        "ENTER_QUERY" => {
            session.system.set_var("MODE", "ENTER-QUERY");
            BuiltinResult::Void
        }
        "CREATE_RECORD" => nav(session.create_record()),
        "DELETE_RECORD" => nav(session.delete_record()),
        "CLEAR_RECORD" => nav(session.clear_record()),
        "CLEAR_BLOCK" => {
            if let Some(block) = session.active_block().map(str::to_string) {
                session.clear_block(&block);
            }
            BuiltinResult::Void
        }
        "CLEAR_FORM" => {
            session.clear_form();
            BuiltinResult::Void
        }
        "COMMIT_FORM" => nav(session.commit()),
        "POST" => BuiltinResult::Unsupported("POST (posting without commit is not supported yet)".into()),
        "EXIT_FORM" => {
            session.exit_requested = true;
            BuiltinResult::Void
        }
        "DO_KEY" => match arg_text(args, 0) {
            Some(key) => nav(session.do_key(&key)),
            None => BuiltinResult::Failure("DO_KEY requires a built-in name".into()),
        },
        "SYNCHRONIZE" => BuiltinResult::Void,
        "BELL" | "PAUSE" | "REDISPLAY" => BuiltinResult::Void,

        // Status
        "FORM_SUCCESS" => BuiltinResult::Value(OracleValue::boolean(!session.last_builtin_failed)),
        "FORM_FAILURE" => BuiltinResult::Value(OracleValue::boolean(session.last_builtin_failed)),
        "FORM_FATAL" => BuiltinResult::Value(OracleValue::boolean(false)),
        "ERROR_TEXT" => BuiltinResult::Value(OracleValue::varchar2(session.last_error_text.clone().unwrap_or_default())),
        "MESSAGE_TEXT" => BuiltinResult::Value(OracleValue::varchar2(session.messages.last().cloned().unwrap_or_default())),

        // Messages & alerts
        "MESSAGE" => {
            let msg = arg_text(args, 0).unwrap_or_default();
            session.messages.push(msg.clone());
            session.trace_log.push(format!("MESSAGE: {}", msg));
            BuiltinResult::Void
        }
        "SHOW_ALERT" => {
            let alert_name = arg_text(args, 0).unwrap_or_default().to_ascii_uppercase();
            let def = session.alerts.get(&alert_name).cloned();
            let Some(def) = def else {
                return Some(BuiltinResult::Failure(format!("FRM-41039: Alert {} not found", alert_name)));
            };
            let request = InteractionRequest::ShowAlert {
                alert_name: alert_name.clone(),
                message: def.message.clone(),
                button1: def.button1.clone(),
                button2: def.button2.clone(),
                button3: def.button3.clone(),
                default_button: def.default_button,
            };
            let button = match session.interaction_handler.as_mut() {
                Some(handler) => match handler(&request) {
                    InteractionResponse::AlertButton(code) => code,
                    InteractionResponse::Acknowledge => ALERT_BUTTON1 + def.default_button.saturating_sub(1) as i64,
                },
                None => {
                    // No user attached (batch run, tests): answer with the default button
                    // and record that this happened.
                    let code = ALERT_BUTTON1 + def.default_button.saturating_sub(1) as i64;
                    session.trace_log.push(format!("ALERT_AUTO_ANSWERED: {} -> button {}", alert_name, def.default_button));
                    code
                }
            };
            BuiltinResult::Value(OracleValue::int(button))
        }
        "SET_ALERT_PROPERTY" => {
            let alert = arg_text(args, 0).unwrap_or_default().to_ascii_uppercase();
            let prop = arg_text(args, 1).unwrap_or_default().to_ascii_uppercase();
            let value = arg_text(args, 2).unwrap_or_default();
            match session.alerts.get_mut(&alert) {
                Some(a) if prop == "ALERT_MESSAGE_TEXT" => {
                    a.message = value;
                    BuiltinResult::Void
                }
                Some(_) => BuiltinResult::Void,
                None => BuiltinResult::Failure(format!("FRM-41039: Alert {} not found", alert)),
            }
        }
        "SET_ALERT_BUTTON_PROPERTY" => BuiltinResult::Void,

        // Dynamic access
        "NAME_IN" => match arg_text(args, 0) {
            Some(var) => BuiltinResult::Value(session.get_bind_value(&var)),
            None => BuiltinResult::Value(OracleValue::Null),
        },
        "COPY" => {
            if let (Some(val), Some(target)) = (args.first(), arg_text(args, 1)) {
                session.set_bind_value(&target, val.clone());
            }
            BuiltinResult::Void
        }
        "DEFAULT_VALUE" => {
            if let (Some(val), Some(target)) = (args.first(), arg_text(args, 1)) {
                if session.get_bind_value(&target).is_null() {
                    session.set_bind_value(&target, val.clone());
                }
            }
            BuiltinResult::Void
        }
        "ERASE" => {
            if let Some(var) = arg_text(args, 0) {
                session.erase_global(&var);
            }
            BuiltinResult::Void
        }

        // Properties
        "SET_ITEM_PROPERTY" | "SET_ITEM_INSTANCE_PROPERTY" => {
            match (arg_text(args, 0), arg_text(args, 1)) {
                (Some(item), Some(prop)) => {
                    let value = args.get(2).cloned().unwrap_or(OracleValue::Null);
                    session.set_item_property(&item, &prop, value);
                    BuiltinResult::Void
                }
                _ => BuiltinResult::Failure(format!("{} requires item, property and value", name_upper)),
            }
        }
        "GET_ITEM_PROPERTY" => match (arg_text(args, 0), arg_text(args, 1)) {
            (Some(item), Some(prop)) => BuiltinResult::Value(session.get_item_property(&item, &prop)),
            _ => BuiltinResult::Value(OracleValue::Null),
        },
        "SET_BLOCK_PROPERTY" => match (arg_text(args, 0), arg_text(args, 1)) {
            (Some(block), Some(prop)) => {
                let value = args.get(2).cloned().unwrap_or(OracleValue::Null);
                session.set_block_property(&block, &prop, value);
                BuiltinResult::Void
            }
            _ => BuiltinResult::Failure("SET_BLOCK_PROPERTY requires block, property and value".into()),
        },
        "GET_BLOCK_PROPERTY" => match (arg_text(args, 0), arg_text(args, 1)) {
            (Some(block), Some(prop)) => BuiltinResult::Value(session.get_block_property(&block, &prop)),
            _ => BuiltinResult::Value(OracleValue::Null),
        },
        "GET_APPLICATION_PROPERTY" => {
            let prop = arg_text(args, 0).unwrap_or_default().to_ascii_uppercase();
            BuiltinResult::Value(match prop.as_str() {
                "USERNAME" => OracleValue::varchar2(session.db_username.clone().unwrap_or_default()),
                "CURRENT_FORM" | "CURRENT_FORM_NAME" => OracleValue::varchar2(session.module.key.name.clone()),
                "OPERATING_SYSTEM" => OracleValue::varchar2(std::env::consts::OS.to_ascii_uppercase()),
                "USER_INTERFACE" => OracleValue::varchar2("WEB"),
                _ => OracleValue::Null,
            })
        }
        // Visual-only properties: accepted so form logic continues; recorded in the trace.
        "SET_WINDOW_PROPERTY" | "SET_CANVAS_PROPERTY" | "SET_VIEW_PROPERTY" | "SET_TAB_PAGE_PROPERTY"
        | "SET_RADIO_BUTTON_PROPERTY" | "SET_LOV_PROPERTY" | "SET_FORM_PROPERTY" | "SHOW_VIEW" | "HIDE_VIEW"
        | "SHOW_WINDOW" | "HIDE_WINDOW" | "SET_APPLICATION_PROPERTY" => {
            session.trace_log.push(format!("UI_PROPERTY_NOT_RENDERED: {}", name_upper));
            BuiltinResult::Void
        }
        "GET_WINDOW_PROPERTY" | "GET_CANVAS_PROPERTY" | "GET_VIEW_PROPERTY" | "GET_FORM_PROPERTY" => {
            BuiltinResult::Value(OracleValue::Null)
        }

        // Object lookup: ids are represented by the object name.
        "FIND_ITEM" | "FIND_BLOCK" | "FIND_ALERT" | "FIND_WINDOW" | "FIND_CANVAS" | "FIND_VIEW" | "FIND_LOV"
        | "FIND_FORM" | "FIND_RECORD_GROUP" | "FIND_TAB_PAGE" | "FIND_RELATION" | "FIND_TIMER" => {
            BuiltinResult::Value(args.first().cloned().unwrap_or(OracleValue::Null))
        }
        "ID_NULL" => BuiltinResult::Value(OracleValue::boolean(args.first().map(|v| v.is_null()).unwrap_or(true))),

        _ => return None,
    };

    if let BuiltinResult::Failure(msg) = &result {
        session.trace_log.push(format!("BUILTIN_FAILED: {}: {}", name_upper, msg));
        session.messages.push(msg.clone());
    } else {
        session.trace_log.push(format!("BUILTIN: {}", name_upper));
    }
    Some(result)
}
