//! Live form sessions: run a form's triggers and PL/SQL on the server and drive it from
//! the browser.
//!
//! Each session owns a `FormSession` on its own thread. Commands arrive over a channel and
//! are run one at a time. When PL/SQL calls `SHOW_ALERT`, the session thread pauses: the
//! pending alert is returned as the HTTP response, and the next `/alert` request carries
//! the user's answer back so the trigger continues exactly where it stopped.
//!
//! Sessions are read-only by default (queries run; DML, locks and commits are refused)
//! and expire after 30 minutes without use.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oracle_forms_core::model::ModuleIndex;
use oracle_forms_db::DbConfig;
use oracle_forms_runtime::{FormSession, InteractionRequest, InteractionResponse, TriggerResult, ALERT_BUTTON1};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::oneshot;

const MAX_SESSIONS: usize = 32;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const ALERT_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_TRACE_LINES: usize = 400;

/// A user action sent from the browser.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionEvent {
    /// WHEN-BUTTON-PRESSED on a push button.
    Button { block: String, item: String, record: Option<usize> },
    /// The user typed a value into an item (then left it: WHEN-VALIDATE-ITEM fires).
    SetItem { block: String, item: String, value: Value, record: Option<usize> },
    /// Move the cursor to an item (fires navigation/validation triggers).
    Navigate { block: String, item: String, record: Option<usize> },
    ExecuteQuery { block: Option<String> },
    NextRecord,
    PreviousRecord,
    CreateRecord,
    DeleteRecord,
    Commit,
    ClearForm,
    /// `DO_KEY(name)`: runs the KEY- trigger if defined, else the default action.
    Key { name: String },
}

type Reply = oneshot::Sender<Value>;

enum Command {
    Event(SessionEvent, Reply),
    State(Reply),
}

struct Shared {
    /// Where the response for the request currently being processed should go.
    reply_slot: Mutex<Option<Reply>>,
    /// Answers to a pending alert: (button constant, reply for the follow-up request).
    answer_tx: Mutex<mpsc::Sender<(i64, Reply)>>,
    awaiting_alert: AtomicBool,
}

pub struct SessionHandle {
    tx: mpsc::Sender<Command>,
    shared: Arc<Shared>,
    last_used: Mutex<Instant>,
}

#[derive(Default, Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Arc<SessionHandle>>>>,
    counter: Arc<AtomicU64>,
}

pub struct StartOptions {
    pub module: ModuleIndex,
    pub libraries: Vec<ModuleIndex>,
    pub missing_libraries: Vec<String>,
    pub db: Option<DbConfig>,
    pub allow_writes: bool,
}

impl SessionManager {
    fn new_id(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        format!("s{}{:08x}", n, nanos ^ (std::process::id().rotate_left(13)))
    }

    /// Drops sessions idle longer than the timeout.
    pub fn reap_idle(&self) {
        let mut map = self.sessions.lock().unwrap();
        map.retain(|_, h| h.last_used.lock().unwrap().elapsed() < IDLE_TIMEOUT);
    }

    pub fn remove(&self, id: &str) -> bool {
        self.sessions.lock().unwrap().remove(id).is_some()
    }

    fn get(&self, id: &str) -> Option<Arc<SessionHandle>> {
        let handle = self.sessions.lock().unwrap().get(id).cloned()?;
        *handle.last_used.lock().unwrap() = Instant::now();
        Some(handle)
    }

    /// Starts a session thread, runs the form startup triggers, and returns the initial
    /// state.
    pub async fn start(&self, opts: StartOptions) -> Result<Value, String> {
        self.reap_idle();
        if self.sessions.lock().unwrap().len() >= MAX_SESSIONS {
            return Err(format!("Too many running sessions (limit {}); close one first", MAX_SESSIONS));
        }

        // Connect (and verify) before spawning anything.
        let driver = match &opts.db {
            Some(cfg) => {
                let d = oracle_forms_db::create_driver(cfg).map_err(|e| e.to_string())?;
                d.test_connection().await.map_err(|e| e.to_string())?;
                Some(Arc::<dyn oracle_forms_db::DatabaseDriver>::from(d))
            }
            None => None,
        };

        let id = self.new_id();
        let (tx, rx) = mpsc::channel::<Command>();
        let (answer_tx, answer_rx) = mpsc::channel::<(i64, Reply)>();
        let shared = Arc::new(Shared {
            reply_slot: Mutex::new(None),
            answer_tx: Mutex::new(answer_tx),
            awaiting_alert: AtomicBool::new(false),
        });
        let (ready_tx, ready_rx) = oneshot::channel::<Value>();

        let thread_shared = shared.clone();
        let session_id = id.clone();
        let db_user = opts.db.as_ref().and_then(|c| c.username.clone());
        let backend = opts.db.as_ref().map(|c| format!("{:?}", c.backend)).unwrap_or_else(|| "none".into());
        std::thread::Builder::new()
            .name(format!("form-session-{}", id))
            .spawn(move || {
                let mut session = FormSession::new(opts.module);
                for lib in &opts.libraries {
                    session.attach_library(lib);
                }
                if let Some(d) = driver {
                    session.set_database_driver(d);
                }
                session.db_username = db_user;
                session.read_only = !opts.allow_writes;

                // SHOW_ALERT: hand the alert to the waiting HTTP request, then block until
                // the browser answers.
                let handler_shared = thread_shared.clone();
                let handler_session_id = session_id.clone();
                let answer_rx = Mutex::new(answer_rx);
                session.set_interaction_handler(Box::new(move |req: &InteractionRequest| {
                    let InteractionRequest::ShowAlert { alert_name, message, button1, button2, button3, default_button } = req;
                    let alert = json!({
                        "sessionId": handler_session_id,
                        "alert": {
                            "alertName": alert_name, "message": message,
                            "button1": button1, "button2": button2, "button3": button3,
                            "defaultButton": default_button,
                        }
                    });
                    handler_shared.awaiting_alert.store(true, Ordering::SeqCst);
                    if let Some(reply) = handler_shared.reply_slot.lock().unwrap().take() {
                        let _ = reply.send(alert);
                    }
                    let answer = answer_rx.lock().unwrap().recv_timeout(ALERT_TIMEOUT);
                    handler_shared.awaiting_alert.store(false, Ordering::SeqCst);
                    match answer {
                        Ok((button, reply)) => {
                            *handler_shared.reply_slot.lock().unwrap() = Some(reply);
                            InteractionResponse::AlertButton(button)
                        }
                        // Abandoned (timeout / session closed): take the default button.
                        Err(_) => InteractionResponse::AlertButton(ALERT_BUTTON1 + (*default_button as i64).saturating_sub(1)),
                    }
                }));

                let mut cursor = Cursors::default();
                *thread_shared.reply_slot.lock().unwrap() = Some(ready_tx);
                let startup_error = session.start().err().map(|e| e.to_string());
                let mut state = snapshot(&session, &session_id, &backend, &mut cursor, &opts.missing_libraries);
                if let Some(e) = startup_error {
                    state["error"] = json!(e);
                }
                if let Some(reply) = thread_shared.reply_slot.lock().unwrap().take() {
                    let _ = reply.send(state);
                }

                while let Ok(cmd) = rx.recv() {
                    let (reply, error) = match cmd {
                        Command::State(reply) => (reply, None),
                        Command::Event(ev, reply) => {
                            *thread_shared.reply_slot.lock().unwrap() = Some(reply);
                            let error = run_event(&mut session, ev);
                            let reply = thread_shared.reply_slot.lock().unwrap().take();
                            match reply {
                                Some(r) => (r, error),
                                None => continue,
                            }
                        }
                    };
                    let mut state = snapshot(&session, &session_id, &backend, &mut cursor, &opts.missing_libraries);
                    if let Some(e) = error {
                        state["error"] = json!(e);
                    }
                    let _ = reply.send(state);
                }
            })
            .map_err(|e| format!("Failed to start session thread: {}", e))?;

        let state = ready_rx.await.map_err(|_| "Session thread exited during startup".to_string())?;
        self.sessions.lock().unwrap().insert(
            id,
            Arc::new(SessionHandle { tx, shared, last_used: Mutex::new(Instant::now()) }),
        );
        Ok(state)
    }

    pub async fn send_event(&self, id: &str, event: SessionEvent) -> Result<Value, (u16, String)> {
        let handle = self.get(id).ok_or((404, "Session not found or expired".to_string()))?;
        if handle.shared.awaiting_alert.load(Ordering::SeqCst) {
            return Err((409, "An alert is waiting for an answer".to_string()));
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .tx
            .send(Command::Event(event, reply_tx))
            .map_err(|_| (410, "Session has ended".to_string()))?;
        reply_rx.await.map_err(|_| (500, "Session thread stopped".to_string()))
    }

    pub async fn answer_alert(&self, id: &str, button: i64) -> Result<Value, (u16, String)> {
        let handle = self.get(id).ok_or((404, "Session not found or expired".to_string()))?;
        if !handle.shared.awaiting_alert.load(Ordering::SeqCst) {
            return Err((409, "No alert is waiting for an answer".to_string()));
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .shared
            .answer_tx
            .lock()
            .unwrap()
            .send((button, reply_tx))
            .map_err(|_| (410, "Session has ended".to_string()))?;
        reply_rx.await.map_err(|_| (500, "Session thread stopped".to_string()))
    }

    pub async fn state(&self, id: &str) -> Result<Value, (u16, String)> {
        let handle = self.get(id).ok_or((404, "Session not found or expired".to_string()))?;
        if handle.shared.awaiting_alert.load(Ordering::SeqCst) {
            return Err((409, "An alert is waiting for an answer".to_string()));
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        handle.tx.send(Command::State(reply_tx)).map_err(|_| (410, "Session has ended".to_string()))?;
        reply_rx.await.map_err(|_| (500, "Session thread stopped".to_string()))
    }
}

/// Runs one browser event against the session. Returns an error message for the UI.
fn run_event(session: &mut FormSession, event: SessionEvent) -> Option<String> {
    let trig = |r: TriggerResult| match r {
        TriggerResult::Success => None,
        TriggerResult::Failure(m) => Some(format!("Trigger failed: {}", m)),
        TriggerResult::UnhandledException(m) => Some(format!("Unhandled exception: {}", m)),
    };
    let rt = |r: Result<(), oracle_forms_runtime::RuntimeError>| r.err().map(|e| e.to_string());
    let goto = |session: &mut FormSession, block: &str, item: &str, record: Option<usize>| -> Option<String> {
        if let Err(e) = session.go_item(&format!("{}.{}", block, item)) {
            return Some(e.to_string());
        }
        if let Some(r) = record {
            let current = session.system.cursor_record;
            if r + 1 != current {
                if let Err(e) = session.go_record(r + 1) {
                    return Some(e.to_string());
                }
            }
        }
        None
    };

    match event {
        SessionEvent::Button { block, item, record } => {
            // Clicking a push button moves input focus to it (Mouse Navigate = Yes, the
            // Forms default), then WHEN-BUTTON-PRESSED fires.
            let _ = session.go_item(&format!("{}.{}", block, item));
            if let Some(r) = record {
                if r + 1 != session.system.cursor_record {
                    let _ = session.go_block(&block);
                    if let Err(e) = session.go_record(r + 1) {
                        return Some(e.to_string());
                    }
                }
            }
            trig(session.press_button(&format!("{}.{}", block, item)))
        }
        SessionEvent::SetItem { block, item, value, record } => {
            if let Some(e) = goto(session, &block, &item, record) {
                return Some(e);
            }
            if let Err(e) = session.set_item_value(&format!("{}.{}", block, item), value) {
                return Some(e.to_string());
            }
            // Leaving the item validates it.
            match session.dispatch_event("WHEN-VALIDATE-ITEM", Some(&item), Some(&block)) {
                TriggerResult::Success => None,
                other => trig(other),
            }
        }
        SessionEvent::Navigate { block, item, record } => goto(session, &block, &item, record),
        SessionEvent::ExecuteQuery { block } => {
            let b = block.or_else(|| session.active_block().map(str::to_string)).unwrap_or_default();
            if let Err(e) = session.go_block(&b) {
                return Some(e.to_string());
            }
            rt(session.do_key("EXECUTE_QUERY"))
        }
        SessionEvent::NextRecord => rt(session.do_key("NEXT_RECORD")),
        SessionEvent::PreviousRecord => rt(session.do_key("PREVIOUS_RECORD")),
        SessionEvent::CreateRecord => rt(session.do_key("CREATE_RECORD")),
        SessionEvent::DeleteRecord => rt(session.do_key("DELETE_RECORD")),
        SessionEvent::Commit => rt(session.do_key("COMMIT_FORM")),
        SessionEvent::ClearForm => rt(session.do_key("CLEAR_FORM")),
        SessionEvent::Key { name } => rt(session.do_key(&name)),
    }
}

/// Tracks how much of the message / trace log the browser has already seen.
#[derive(Default)]
struct Cursors {
    messages: usize,
    trace: usize,
}

fn snapshot(session: &FormSession, id: &str, backend: &str, cursor: &mut Cursors, missing_libraries: &[String]) -> Value {
    let blocks: Vec<Value> = session
        .module
        .blocks
        .iter()
        .map(|b| {
            let buf = session.blocks.get(&b.name.to_ascii_uppercase());
            let records: Vec<Value> = buf
                .map(|buf| {
                    buf.records
                        .iter()
                        .map(|r| json!({ "status": r.status.as_str(), "values": r.values }))
                        .collect()
                })
                .unwrap_or_default();
            json!({
                "name": b.name,
                "currentRecord": buf.map(|b| b.current_record_index).unwrap_or(0),
                "status": buf.map(|b| b.status().as_str()).unwrap_or("NEW"),
                "pendingDeletes": buf.map(|b| b.deleted_records.len()).unwrap_or(0),
                "records": records,
            })
        })
        .collect();

    let new_messages: Vec<&String> = session.messages.iter().skip(cursor.messages).collect();
    cursor.messages = session.messages.len();
    let start = cursor.trace.max(session.trace_log.len().saturating_sub(MAX_TRACE_LINES));
    let new_trace: Vec<&String> = session.trace_log.iter().skip(start).collect();
    cursor.trace = session.trace_log.len();

    let mut properties = serde_json::Map::new();
    for (item, props) in &session.item_properties {
        let mut m = serde_json::Map::new();
        for (p, v) in props {
            m.insert(p.clone(), v.to_json());
        }
        properties.insert(item.clone(), Value::Object(m));
    }
    json!({
        "sessionId": id,
        "moduleName": session.module.key.name,
        "backend": backend,
        "readOnly": session.read_only,
        "cursorBlock": session.system.cursor_block,
        "cursorItem": session.system.cursor_item,
        "cursorRecord": session.system.cursor_record,
        "formStatus": session.system.form_status.as_str(),
        "blocks": blocks,
        "itemProperties": properties,
        "messages": new_messages,
        "trace": new_trace,
        "exitRequested": session.exit_requested,
        "missingLibraries": missing_libraries,
        "alert": Value::Null,
        "error": Value::Null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_forms_core::model::{ModuleKey, ModuleType};
    use std::path::PathBuf;

    fn conformance(name: &str) -> ModuleIndex {
        let path = PathBuf::from("../../samples/conformance").join(name);
        let cache = std::env::temp_dir().join(format!("ofw-sess-{}", std::process::id()));
        oracle_forms_xml::FormsXmlParser::parse(&ModuleKey::new("CONF", ModuleType::Form), &path, &cache).unwrap()
    }

    fn sqlite() -> Option<DbConfig> {
        Some(DbConfig { backend: oracle_forms_db::DbBackend::Sqlite, database_path: Some(":memory:".into()), ..Default::default() })
    }

    #[tokio::test]
    async fn alert_pauses_trigger_and_answer_resumes_it() {
        let mgr = SessionManager::default();
        let state = mgr
            .start(StartOptions { module: conformance("conf_03_query_cascade_fmb.xml"), libraries: vec![], missing_libraries: vec![], db: sqlite(), allow_writes: false })
            .await
            .unwrap();
        let id = state["sessionId"].as_str().unwrap().to_string();

        let paused = mgr
            .send_event(&id, SessionEvent::Button { block: "ORDERS".into(), item: "BTN_DELETE".into(), record: None })
            .await
            .unwrap();
        assert_eq!(paused["alert"]["alertName"], "CONFIRM_DELETE", "{paused}");
        assert_eq!(paused["sessionId"], id);

        // While the alert is open, other events are refused.
        let busy = mgr.send_event(&id, SessionEvent::NextRecord).await.unwrap_err();
        assert_eq!(busy.0, 409);

        let done = mgr.answer_alert(&id, ALERT_BUTTON1).await.unwrap();
        assert!(done["alert"].is_null(), "{done}");
        let orders = done["blocks"].as_array().unwrap().iter().find(|b| b["name"] == "ORDERS").unwrap();
        assert_eq!(orders["records"][0]["values"]["ORDER_STATUS"], "CANCELLED");
        assert!(done["messages"].as_array().unwrap().iter().any(|m| m == "Order cancelled."), "{done}");
        assert!(mgr.remove(&id));
    }

    #[tokio::test]
    async fn read_only_session_refuses_commit_but_queries_run() {
        let mgr = SessionManager::default();
        let state = mgr
            .start(StartOptions { module: conformance("conf_08_commit_pipeline_fmb.xml"), libraries: vec![], missing_libraries: vec![], db: sqlite(), allow_writes: false })
            .await
            .unwrap();
        let id = state["sessionId"].as_str().unwrap().to_string();
        let q = mgr.send_event(&id, SessionEvent::ExecuteQuery { block: Some("ITEMS".into()) }).await.unwrap();
        assert!(q["error"].is_null(), "{q}");
        let items = q["blocks"].as_array().unwrap().iter().find(|b| b["name"] == "ITEMS").unwrap();
        assert!(items["records"].as_array().unwrap().len() >= 2, "{items}");

        let edited = mgr
            .send_event(&id, SessionEvent::SetItem { block: "ITEMS".into(), item: "ITEM_NAME".into(), value: json!("changed"), record: Some(0) })
            .await
            .unwrap();
        assert!(edited["error"].is_null(), "{edited}");
        let commit = mgr.send_event(&id, SessionEvent::Commit).await.unwrap();
        assert!(commit["error"].as_str().unwrap_or("").contains("read-only"), "{commit}");
    }
}
