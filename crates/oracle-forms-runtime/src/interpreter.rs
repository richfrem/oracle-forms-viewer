//! Tree-walking interpreter for Forms-side PL/SQL.
//!
//! What runs here:
//! * trigger bodies, form program units, packages, and attached-library units, with
//!   IN / OUT / IN OUT parameters, functions, package state, nested blocks and scopes;
//! * Forms built-ins ([`crate::builtins`]) and common SQL functions ([`crate::stdfuncs`]);
//! * embedded SQL (`SELECT INTO`, DML, explicit cursors, cursor FOR loops), sent to the
//!   attached database with every `:BLOCK.ITEM` and local variable passed as a bind;
//! * PL/SQL exceptions (`NO_DATA_FOUND`, `TOO_MANY_ROWS`, `VALUE_ERROR`, user-defined,
//!   `FORM_TRIGGER_FAILURE`), caught by `EXCEPTION WHEN ...` handlers, plus
//!   `SQLCODE` / `SQLERRM`.
//!
//! What does not run is reported, never silently skipped: unparsed statements, unknown
//! identifiers and unsupported built-ins stop the trigger with a clear error.
//!
//! `SHOW_ALERT` asks the session's interaction handler synchronously, so an alert can
//! appear anywhere (inside loops, handlers, or the commit pipeline) and execution
//! continues exactly where it left off.

use crate::builtins::{self, BuiltinResult, ALERT_BUTTON1, ALERT_BUTTON2, ALERT_BUTTON3, PROPERTY_FALSE, PROPERTY_TRUE};
use crate::program_units::Routine;
use crate::session::{block_on_future, FormSession, RuntimeError};
use crate::stdfuncs;
use crate::value::{OracleValue, ValueError};
use oracle_forms_db::{rewrite_binds, BoundSql, DbBackend, QueryResult};
use oracle_forms_plsql::ast::{
    BlockStmt, CursorDecl, Declaration, DmlKind, ExceptionHandler, Expr, ParamMode, ParameterDecl, PlsqlUnit,
    SqlStatement, Statement, VariableDecl,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InteractionRequest {
    ShowAlert {
        alert_name: String,
        message: String,
        button1: String,
        button2: Option<String>,
        button3: Option<String>,
        default_button: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InteractionResponse {
    /// `ALERT_BUTTON1` / `2` / `3` constant value.
    AlertButton(i64),
    /// Accept the default.
    Acknowledge,
}

/// Outcome of running a unit of PL/SQL.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStatus {
    Success(Option<OracleValue>),
    /// `FORM_TRIGGER_FAILURE` reached the top of the trigger.
    TriggerFailure(String),
    /// Any other exception reached the top of the trigger (`name: message`).
    UnhandledException(String),
}

impl ExecutionStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, ExecutionStatus::Success(_))
    }
}

/// A raised PL/SQL exception.
#[derive(Debug, Clone, PartialEq)]
pub struct RaisedException {
    pub name: String,
    pub message: String,
    pub code: i64,
}

impl RaisedException {
    fn new(name: &str, code: i64, message: impl Into<String>) -> Self {
        Self { name: name.to_ascii_uppercase(), code, message: message.into() }
    }

    fn from_value_error(e: ValueError) -> Self {
        let code = if e.exception == "ZERO_DIVIDE" { -1476 } else { -6502 };
        Self::new(e.exception, code, e.message)
    }

    /// Maps a database error message to the predefined exception PL/SQL would raise.
    fn from_db_error(message: &str) -> Self {
        let code = message
            .find("ORA-")
            .and_then(|i| message[i + 4..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<i64>().ok())
            .map(|n| -n)
            .unwrap_or(-20000);
        let name = match code {
            -1 => "DUP_VAL_ON_INDEX",
            -1403 => "NO_DATA_FOUND",
            -1422 => "TOO_MANY_ROWS",
            -1476 => "ZERO_DIVIDE",
            -1722 => "INVALID_NUMBER",
            -54 => "RESOURCE_BUSY",
            -1001 => "INVALID_CURSOR",
            _ => "ORACLE_ERROR",
        };
        Self::new(name, code, message.to_string())
    }
}

/// Control flow out of a statement.
#[derive(Debug, Clone)]
enum Flow {
    Normal,
    Exit,
    Return(Option<OracleValue>),
    Raise(RaisedException),
}

type ExecResult = Result<Flow, RuntimeError>;

/// An open explicit cursor (rows are fetched eagerly when opened).
#[derive(Debug, Clone, Default)]
struct OpenCursor {
    columns: Vec<String>,
    rows: Vec<HashMap<String, serde_json::Value>>,
    position: usize,
    found: Option<bool>,
}

#[derive(Debug, Clone, Default)]
struct Frame {
    vars: HashMap<String, OracleValue>,
    /// Declared data type per variable (for assignment conversions).
    types: HashMap<String, String>,
    cursor_decls: HashMap<String, CursorDecl>,
    open_cursors: HashMap<String, OpenCursor>,
    /// A routine call boundary: variable lookup does not continue past it.
    boundary: bool,
    /// Package whose routine owns this frame (for unqualified sibling resolution).
    package: Option<String>,
}

pub struct Interpreter {
    pub step_count: usize,
    pub max_steps: usize,
    pub max_depth: usize,
    frames: Vec<Frame>,
    /// Exceptions currently being handled (innermost last): for `RAISE;`, `SQLERRM`.
    handling: Vec<RaisedException>,
    sql_rowcount: Option<i64>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self {
            step_count: 0,
            max_steps: 100_000,
            max_depth: 64,
            frames: vec![Frame { boundary: true, ..Default::default() }],
            handling: Vec::new(),
            sql_rowcount: None,
        }
    }
}

fn unsupported(what: impl Into<String>) -> RuntimeError {
    RuntimeError::Unsupported(what.into())
}

/// Splits `NAME` / `PKG.NAME` into an upper-case name.
fn upper(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}

fn dotted(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(s) => Some(s.clone()),
        Expr::MemberAccess { object, member } => dotted(object).map(|o| format!("{}.{}", o, member)),
        _ => None,
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self::default()
    }

    // ------------------------------------------------------------------
    // Entry points
    // ------------------------------------------------------------------

    /// Runs a trigger body or anonymous unit.
    pub fn execute_unit(&mut self, unit: &PlsqlUnit, session: &mut FormSession) -> Result<ExecutionStatus, RuntimeError> {
        let flow = match unit {
            PlsqlUnit::AnonymousBlock(b) => self.exec_block(b, session)?,
            PlsqlUnit::StatementList(stmts) => self.exec_statements(stmts, session)?,
            PlsqlUnit::Procedure(_) | PlsqlUnit::Function(_) | PlsqlUnit::PackageSpec(_) | PlsqlUnit::PackageBody(_) => {
                return Err(unsupported("a program unit declaration cannot be executed as a trigger body"));
            }
        };
        Ok(match flow {
            Flow::Normal | Flow::Exit => ExecutionStatus::Success(None),
            Flow::Return(v) => ExecutionStatus::Success(v),
            Flow::Raise(e) if e.name == "FORM_TRIGGER_FAILURE" => ExecutionStatus::TriggerFailure(e.message),
            Flow::Raise(e) => ExecutionStatus::UnhandledException(format!("{}: {}", e.name, e.message)),
        })
    }

    /// Calls a named program unit with already-evaluated IN arguments (used by tools and
    /// tests). OUT parameters are discarded.
    pub fn call_procedure(&mut self, name: &str, args: Vec<OracleValue>, session: &mut FormSession) -> Result<ExecutionStatus, RuntimeError> {
        let exprs: Vec<Expr> = args
            .into_iter()
            .map(|v| Expr::Literal(match v {
                OracleValue::Null => oracle_forms_plsql::ast::LiteralValue::Null,
                OracleValue::Number(n) => oracle_forms_plsql::ast::LiteralValue::Number(n.to_string()),
                OracleValue::Boolean(Some(b)) => oracle_forms_plsql::ast::LiteralValue::Boolean(b),
                OracleValue::Boolean(None) => oracle_forms_plsql::ast::LiteralValue::Null,
                other => oracle_forms_plsql::ast::LiteralValue::String(other.to_string_lossy()),
            }))
            .collect();
        match self.call(name, &exprs, session, false)? {
            Ok(v) => Ok(ExecutionStatus::Success(v)),
            Err(e) if e.name == "FORM_TRIGGER_FAILURE" => Ok(ExecutionStatus::TriggerFailure(e.message)),
            Err(e) => Ok(ExecutionStatus::UnhandledException(format!("{}: {}", e.name, e.message))),
        }
    }

    // ------------------------------------------------------------------
    // Scopes
    // ------------------------------------------------------------------

    fn tick(&mut self) -> Result<(), RuntimeError> {
        self.step_count += 1;
        if self.step_count > self.max_steps {
            Err(RuntimeError::StepBudgetExceeded)
        } else {
            Ok(())
        }
    }

    fn current_package(&self) -> Option<String> {
        self.frames.iter().rev().find(|f| f.boundary).and_then(|f| f.package.clone())
    }

    /// Visible frames, innermost first, up to and including the routine boundary.
    fn visible_frames(&self) -> impl Iterator<Item = &Frame> {
        let boundary = self.frames.iter().rposition(|f| f.boundary).unwrap_or(0);
        self.frames[boundary..].iter().rev()
    }

    fn lookup_var(&self, name: &str) -> Option<OracleValue> {
        let key = upper(name);
        self.visible_frames().find_map(|f| f.vars.get(&key).cloned())
    }

    fn set_var(&mut self, name: &str, value: OracleValue) -> bool {
        let key = upper(name);
        let boundary = self.frames.iter().rposition(|f| f.boundary).unwrap_or(0);
        for f in self.frames[boundary..].iter_mut().rev() {
            if f.vars.contains_key(&key) {
                let value = match f.types.get(&key) {
                    Some(t) => value.coerce_to(t),
                    None => value,
                };
                f.vars.insert(key, value);
                return true;
            }
        }
        false
    }

    fn declare_typed(&mut self, name: &str, value: OracleValue, data_type: &oracle_forms_plsql::ast::DataType) {
        let ty = match data_type {
            oracle_forms_plsql::ast::DataType::Primitive(t) | oracle_forms_plsql::ast::DataType::Custom(t) => Some(t.clone()),
            _ => None,
        };
        if let Some(f) = self.frames.last_mut() {
            let key = upper(name);
            let value = match &ty {
                Some(t) => value.coerce_to(t),
                None => value,
            };
            if let Some(t) = ty {
                f.types.insert(key.clone(), t);
            }
            f.vars.insert(key, value);
        }
    }

    fn declare(&mut self, name: &str, value: OracleValue) {
        if let Some(f) = self.frames.last_mut() {
            f.vars.insert(upper(name), value);
        }
    }

    fn find_cursor_decl(&self, name: &str) -> Option<CursorDecl> {
        let key = upper(name);
        self.visible_frames().find_map(|f| f.cursor_decls.get(&key).cloned())
    }

    fn open_cursor_mut(&mut self, name: &str) -> Option<&mut OpenCursor> {
        let key = upper(name);
        let boundary = self.frames.iter().rposition(|f| f.boundary).unwrap_or(0);
        self.frames[boundary..].iter_mut().rev().find_map(|f| f.open_cursors.get_mut(&key))
    }

    fn package_var_key(&self, name: &str, session: &FormSession) -> Option<String> {
        let n = upper(name);
        if n.contains('.') {
            let (pkg, _) = n.split_once('.').unwrap();
            if session.programs.is_package(pkg) {
                return Some(n);
            }
            return None;
        }
        self.current_package().map(|p| format!("{}.{}", p, n))
    }

    /// Initialises a package's variables on first use (Forms instantiates package state
    /// once per session).
    fn ensure_package(&mut self, pkg: &str, session: &mut FormSession) -> Result<Option<RaisedException>, RuntimeError> {
        let pkg = upper(pkg);
        if session.initialized_packages.contains(&pkg) {
            return Ok(None);
        }
        session.initialized_packages.insert(pkg.clone());
        let vars: Vec<VariableDecl> = session.programs.package(&pkg).map(|p| p.variables.clone()).unwrap_or_default();
        self.frames.push(Frame { boundary: true, package: Some(pkg.clone()), ..Default::default() });
        let mut raised = None;
        for v in vars {
            let value = match &v.default_value {
                Some(e) => match self.eval(e, session)? {
                    Ok(val) => val,
                    Err(ex) => {
                        raised = Some(ex);
                        break;
                    }
                },
                None => OracleValue::Null,
            };
            session.package_vars.insert(format!("{}.{}", pkg, upper(&v.name)), value);
        }
        self.frames.pop();
        Ok(raised)
    }

    // ------------------------------------------------------------------
    // Blocks and statements
    // ------------------------------------------------------------------

    fn declare_all(&mut self, decls: &[Declaration], session: &mut FormSession) -> Result<Option<RaisedException>, RuntimeError> {
        for d in decls {
            match d {
                Declaration::Variable(v) | Declaration::Constant(v) => {
                    let ty = format!("{:?}", v.data_type).to_ascii_uppercase();
                    if ty.contains("\"EXCEPTION\"") {
                        continue; // user-defined exception name; nothing to store
                    }
                    if v.name.contains(' ') || ty.contains("UNKNOWN") {
                        return Err(unsupported(format!("declaration not understood: {}", v.name)));
                    }
                    let value = match &v.default_value {
                        Some(e) => match self.eval(e, session)? {
                            Ok(val) => val,
                            Err(ex) => return Ok(Some(ex)),
                        },
                        None => OracleValue::Null,
                    };
                    self.declare_typed(&v.name, value, &v.data_type);
                }
                Declaration::Cursor(c) => {
                    if let Some(f) = self.frames.last_mut() {
                        f.cursor_decls.insert(upper(&c.name), c.clone());
                    }
                }
            }
        }
        Ok(None)
    }

    fn exec_block(&mut self, block: &BlockStmt, session: &mut FormSession) -> ExecResult {
        self.frames.push(Frame::default());
        let result = (|| -> ExecResult {
            if let Some(ex) = self.declare_all(&block.declarations, session)? {
                // Exceptions in the declaration section propagate to the enclosing block.
                return Ok(Flow::Raise(ex));
            }
            let flow = self.exec_statements(&block.body, session)?;
            self.handle(flow, &block.exception_handlers, session)
        })();
        self.frames.pop();
        result
    }

    /// Runs the matching exception handler, if any, for a raised exception.
    fn handle(&mut self, flow: Flow, handlers: &[ExceptionHandler], session: &mut FormSession) -> ExecResult {
        let Flow::Raise(ex) = flow else { return Ok(flow) };
        let handler = handlers.iter().find(|h| {
            h.exceptions.iter().any(|name| {
                let n = upper(name);
                n == ex.name || n == "OTHERS"
            })
        });
        match handler {
            Some(h) => {
                self.handling.push(ex);
                let result = self.exec_statements(&h.body, session);
                self.handling.pop();
                result
            }
            None => Ok(Flow::Raise(ex)),
        }
    }

    fn exec_statements(&mut self, stmts: &[Statement], session: &mut FormSession) -> ExecResult {
        for stmt in stmts {
            self.tick()?;
            match self.exec_statement(stmt, session)? {
                Flow::Normal => continue,
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    /// Runs a loop body once; `Ok(true)` means keep looping.
    fn loop_iteration(&mut self, body: &[Statement], session: &mut FormSession) -> Result<Result<bool, Flow>, RuntimeError> {
        self.tick()?;
        Ok(match self.exec_statements(body, session)? {
            Flow::Normal => Ok(true),
            Flow::Exit => Ok(false),
            other => Err(other),
        })
    }

    fn exec_statement(&mut self, stmt: &Statement, session: &mut FormSession) -> ExecResult {
        match stmt {
            Statement::Null => Ok(Flow::Normal),

            Statement::Assignment { target, value } => {
                let v = match self.eval(value, session)? {
                    Ok(v) => v,
                    Err(ex) => return Ok(Flow::Raise(ex)),
                };
                self.assign(target, v, session)
            }

            Statement::Call { target, args } => match self.call(target, args, session, false)? {
                Ok(_) => Ok(Flow::Normal),
                Err(ex) => Ok(Flow::Raise(ex)),
            },

            Statement::If { condition, then_branch, elsifs, else_branch } => {
                match self.eval(condition, session)? {
                    Ok(c) if c.is_truthy() => return self.exec_statements(then_branch, session),
                    Ok(_) => {}
                    Err(ex) => return Ok(Flow::Raise(ex)),
                }
                for (cond, branch) in elsifs {
                    match self.eval(cond, session)? {
                        Ok(c) if c.is_truthy() => return self.exec_statements(branch, session),
                        Ok(_) => {}
                        Err(ex) => return Ok(Flow::Raise(ex)),
                    }
                }
                match else_branch {
                    Some(b) => self.exec_statements(b, session),
                    None => Ok(Flow::Normal),
                }
            }

            Statement::Case { selector, whens, else_branch } => {
                let sel = match selector {
                    Some(s) => match self.eval(s, session)? {
                        Ok(v) => Some(v),
                        Err(ex) => return Ok(Flow::Raise(ex)),
                    },
                    None => None,
                };
                for (cond, body) in whens {
                    let c = match self.eval(cond, session)? {
                        Ok(v) => v,
                        Err(ex) => return Ok(Flow::Raise(ex)),
                    };
                    let hit = match &sel {
                        Some(s) => match s.eval_binary_op(oracle_forms_plsql::ast::BinaryOp::Equal, &c) {
                            Ok(b) => b.is_truthy(),
                            Err(e) => return Ok(Flow::Raise(RaisedException::from_value_error(e))),
                        },
                        None => c.is_truthy(),
                    };
                    if hit {
                        return self.exec_statements(body, session);
                    }
                }
                match else_branch {
                    Some(b) => self.exec_statements(b, session),
                    None => Ok(Flow::Raise(RaisedException::new("CASE_NOT_FOUND", -6592, "ORA-06592: CASE not found while executing CASE statement"))),
                }
            }

            Statement::Loop { body } => loop {
                match self.loop_iteration(body, session)? {
                    Ok(true) => continue,
                    Ok(false) => return Ok(Flow::Normal),
                    Err(flow) => return Ok(flow),
                }
            },

            Statement::WhileLoop { condition, body } => loop {
                match self.eval(condition, session)? {
                    Ok(c) if c.is_truthy() => {}
                    Ok(_) => return Ok(Flow::Normal),
                    Err(ex) => return Ok(Flow::Raise(ex)),
                }
                match self.loop_iteration(body, session)? {
                    Ok(true) => continue,
                    Ok(false) => return Ok(Flow::Normal),
                    Err(flow) => return Ok(flow),
                }
            },

            Statement::ForLoop { var_name, reverse, lower, upper: hi, body } => {
                let lo_v = match self.eval(lower, session)? { Ok(v) => v, Err(ex) => return Ok(Flow::Raise(ex)) };
                let hi_v = match self.eval(hi, session)? { Ok(v) => v, Err(ex) => return Ok(Flow::Raise(ex)) };
                let (Some(lo), Some(hi)) = (lo_v.as_i64(), hi_v.as_i64()) else {
                    return Ok(Flow::Raise(RaisedException::new("VALUE_ERROR", -6502, "ORA-06502: FOR loop bounds must be numbers")));
                };
                let range: Box<dyn Iterator<Item = i64>> = if *reverse { Box::new((lo..=hi).rev()) } else { Box::new(lo..=hi) };
                self.frames.push(Frame::default());
                let mut result = Ok(Flow::Normal);
                for i in range {
                    self.declare(var_name, OracleValue::int(i));
                    match self.loop_iteration(body, session) {
                        Ok(Ok(true)) => continue,
                        Ok(Ok(false)) => break,
                        Ok(Err(flow)) => {
                            result = Ok(flow);
                            break;
                        }
                        Err(e) => {
                            result = Err(e);
                            break;
                        }
                    }
                }
                self.frames.pop();
                result
            }

            Statement::Exit { condition } => match condition {
                None => Ok(Flow::Exit),
                Some(c) => match self.eval(c, session)? {
                    Ok(v) if v.is_truthy() => Ok(Flow::Exit),
                    Ok(_) => Ok(Flow::Normal),
                    Err(ex) => Ok(Flow::Raise(ex)),
                },
            },

            Statement::Return(expr) => match expr {
                None => Ok(Flow::Return(None)),
                Some(e) => match self.eval(e, session)? {
                    Ok(v) => Ok(Flow::Return(Some(v))),
                    Err(ex) => Ok(Flow::Raise(ex)),
                },
            },

            Statement::Raise(name) => match name {
                Some(n) => {
                    let n = upper(n);
                    let (code, msg) = match n.as_str() {
                        "FORM_TRIGGER_FAILURE" => (-6510, "FORM_TRIGGER_FAILURE".to_string()),
                        "NO_DATA_FOUND" => (100, "ORA-01403: no data found".to_string()),
                        "TOO_MANY_ROWS" => (-1422, "ORA-01422: exact fetch returns more than requested number of rows".to_string()),
                        "VALUE_ERROR" => (-6502, "ORA-06502: PL/SQL: numeric or value error".to_string()),
                        "ZERO_DIVIDE" => (-1476, "ORA-01476: divisor is equal to zero".to_string()),
                        _ => (1, "User-Defined Exception".to_string()),
                    };
                    Ok(Flow::Raise(RaisedException::new(&n, code, msg)))
                }
                None => match self.handling.last() {
                    Some(ex) => Ok(Flow::Raise(ex.clone())),
                    None => Err(unsupported("RAISE; outside an exception handler")),
                },
            },

            Statement::Block(b) => self.exec_block(b, session),

            Statement::Sql(sql) => self.exec_sql(sql, session),

            Statement::OpenCursor { cursor, args } => {
                let decl = self.find_cursor_decl(cursor).ok_or_else(|| unsupported(format!("cursor {} is not declared", cursor)))?;
                if self.open_cursor_mut(cursor).is_some() {
                    return Ok(Flow::Raise(RaisedException::new("CURSOR_ALREADY_OPEN", -6511, "ORA-06511: PL/SQL: cursor already open")));
                }
                let result = match self.run_cursor_query(&decl, args, session)? {
                    Ok(r) => r,
                    Err(ex) => return Ok(Flow::Raise(ex)),
                };
                let state = OpenCursor {
                    columns: result.columns.iter().map(|c| upper(c)).collect(),
                    rows: result.records.into_iter().map(|r| r.values).collect(),
                    position: 0,
                    found: None,
                };
                // Store the open cursor in the frame that declared it.
                let key = upper(cursor);
                let boundary = self.frames.iter().rposition(|f| f.boundary).unwrap_or(0);
                if let Some(f) = self.frames[boundary..].iter_mut().rev().find(|f| f.cursor_decls.contains_key(&key)) {
                    f.open_cursors.insert(key, state);
                }
                Ok(Flow::Normal)
            }

            Statement::FetchCursor { cursor, into } => {
                let Some(state) = self.open_cursor_mut(cursor) else {
                    return Ok(Flow::Raise(RaisedException::new("INVALID_CURSOR", -1001, "ORA-01001: invalid cursor")));
                };
                if state.position >= state.rows.len() {
                    state.found = Some(false);
                    return Ok(Flow::Normal);
                }
                let row = state.rows[state.position].clone();
                let columns = state.columns.clone();
                state.position += 1;
                state.found = Some(true);
                self.assign_row(into, &columns, &row, session)
            }

            Statement::CloseCursor { cursor } => {
                let key = upper(cursor);
                let boundary = self.frames.iter().rposition(|f| f.boundary).unwrap_or(0);
                let closed = self.frames[boundary..].iter_mut().rev().any(|f| f.open_cursors.remove(&key).is_some());
                if closed {
                    Ok(Flow::Normal)
                } else {
                    Ok(Flow::Raise(RaisedException::new("INVALID_CURSOR", -1001, "ORA-01001: invalid cursor")))
                }
            }

            Statement::CursorForLoop { var_name, cursor, args, query, body } => {
                let result = match (cursor, query) {
                    (Some(c), _) => {
                        let decl = self.find_cursor_decl(c).ok_or_else(|| unsupported(format!("cursor {} is not declared", c)))?;
                        self.run_cursor_query(&decl, args, session)?
                    }
                    (None, Some(q)) => self.run_query(q, session)?,
                    _ => return Err(unsupported("cursor FOR loop without a cursor")),
                };
                let result = match result {
                    Ok(r) => r,
                    Err(ex) => return Ok(Flow::Raise(ex)),
                };
                let columns: Vec<String> = result.columns.iter().map(|c| upper(c)).collect();
                let record = upper(var_name);
                let mut outcome = Ok(Flow::Normal);
                for row in result.records {
                    self.frames.push(Frame::default());
                    for col in &columns {
                        let v = row.values.get(col).map(OracleValue::from_json).unwrap_or(OracleValue::Null);
                        self.declare(&format!("{}.{}", record, col), v);
                    }
                    let step = self.loop_iteration(body, session);
                    self.frames.pop();
                    match step {
                        Ok(Ok(true)) => continue,
                        Ok(Ok(false)) => break,
                        Ok(Err(flow)) => {
                            outcome = Ok(flow);
                            break;
                        }
                        Err(e) => {
                            outcome = Err(e);
                            break;
                        }
                    }
                }
                outcome
            }

            Statement::Opaque { raw, error } => Err(unsupported(format!(
                "statement could not be parsed ({}): {}",
                error.clone().unwrap_or_else(|| "unrecognised".into()),
                raw
            ))),
        }
    }

    // ------------------------------------------------------------------
    // Assignment
    // ------------------------------------------------------------------

    fn assign(&mut self, target: &Expr, value: OracleValue, session: &mut FormSession) -> ExecResult {
        match target {
            Expr::Bind(name) => {
                session.set_bind_value(name, value);
                Ok(Flow::Normal)
            }
            Expr::Identifier(_) | Expr::MemberAccess { .. } => {
                let name = dotted(target).unwrap_or_default();
                if self.set_var(&name, value.clone()) {
                    return Ok(Flow::Normal);
                }
                if let Some(key) = self.package_var_key(&name, session) {
                    let pkg = key.split('.').next().unwrap_or_default().to_string();
                    if let Some(ex) = self.ensure_package(&pkg, session)? {
                        return Ok(Flow::Raise(ex));
                    }
                    if let Some(slot) = session.package_vars.get_mut(&key) {
                        *slot = value;
                        return Ok(Flow::Normal);
                    }
                }
                Err(unsupported(format!("PLS-00201: identifier '{}' must be declared", name)))
            }
            other => Err(unsupported(format!("cannot assign to {:?}", other))),
        }
    }

    /// Assigns a fetched row to INTO targets: by position, or field-by-field into a single
    /// record variable (`%ROWTYPE`).
    fn assign_row(&mut self, into: &[Expr], columns: &[String], row: &HashMap<String, serde_json::Value>, session: &mut FormSession) -> ExecResult {
        if into.len() == 1 && columns.len() > 1 {
            if let Some(rec) = dotted(&into[0]) {
                for col in columns {
                    let v = row.get(col).map(OracleValue::from_json).unwrap_or(OracleValue::Null);
                    let field = format!("{}.{}", upper(&rec), col);
                    if !self.set_var(&field, v.clone()) {
                        self.declare(&field, v);
                    }
                }
                return Ok(Flow::Normal);
            }
        }
        if into.len() != columns.len() {
            return Err(unsupported(format!(
                "INTO list has {} targets but the query returns {} columns",
                into.len(),
                columns.len()
            )));
        }
        for (target, col) in into.iter().zip(columns) {
            let v = row.get(col).map(OracleValue::from_json).unwrap_or(OracleValue::Null);
            if let Flow::Raise(ex) = self.assign(target, v, session)? {
                return Ok(Flow::Raise(ex));
            }
        }
        Ok(Flow::Normal)
    }

    // ------------------------------------------------------------------
    // Embedded SQL
    // ------------------------------------------------------------------

    fn backend(session: &FormSession) -> Option<DbBackend> {
        session.database_driver.as_ref().map(|d| d.backend())
    }

    /// Turns PL/SQL-embedded SQL into bound SQL: `:BLOCK.ITEM` references and local
    /// variables become bind placeholders with their current values.
    fn prepare_sql(&mut self, sql: &str, session: &mut FormSession) -> Result<Result<BoundSql, RaisedException>, RuntimeError> {
        let backend = Self::backend(session).ok_or_else(|| unsupported("embedded SQL needs an attached database"))?;
        let rewritten = rewrite_binds(sql, backend, 1);
        let mut binds = Vec::new();
        for r in &rewritten.refs {
            let value = session.get_bind_value(&r.reference);
            binds.push((r.placeholder.clone(), value.to_sql_value()));
        }
        let mut next = rewritten.refs.len() + 1;
        let (sql, local_binds) = self.bind_local_variables(&rewritten.sql, backend, &mut next, session)?;
        binds.extend(local_binds);
        Ok(Ok(BoundSql { sql, binds }))
    }

    /// Replaces identifiers that name visible PL/SQL variables (outside literals, comments
    /// and qualified names) with bind placeholders.
    ///
    /// Note: Oracle gives a *column* precedence over a same-named variable. This runtime
    /// cannot see table definitions, so it binds the variable; name variables distinctly
    /// (the usual `v_` / `p_` convention) to avoid ambiguity.
    fn bind_local_variables(
        &mut self,
        sql: &str,
        backend: DbBackend,
        next: &mut usize,
        session: &mut FormSession,
    ) -> Result<(String, Vec<(String, oracle_forms_db::SqlValue)>), RuntimeError> {
        let chars: Vec<char> = sql.chars().collect();
        let mut out = String::with_capacity(sql.len());
        let mut binds = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            // Skip literals, quoted identifiers and comments verbatim.
            if c == '\'' || c == '"' {
                let q = c;
                out.push(c);
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == q {
                        if chars.get(i + 1) == Some(&q) {
                            out.push(q);
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            if c == '-' && chars.get(i + 1) == Some(&'-') {
                while i < chars.len() && chars[i] != '\n' {
                    out.push(chars[i]);
                    i += 1;
                }
                continue;
            }
            if c == '/' && chars.get(i + 1) == Some(&'*') {
                let start = i;
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
                out.extend(&chars[start..i]);
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$' | '#' | '.')) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let prev = out.trim_end().chars().last();
                let next_char = chars[i..].iter().find(|ch| !ch.is_whitespace()).copied();
                let qualified_or_bind = matches!(prev, Some('.') | Some(':'));
                let is_call = next_char == Some('(');
                if !qualified_or_bind && !is_call {
                    let value = if let Some(v) = self.lookup_var(&word) {
                        Some(v)
                    } else if let Some(key) = self.package_var_key(&word, session) {
                        session.package_vars.get(&key).cloned()
                    } else {
                        None
                    };
                    if let Some(v) = value {
                        let placeholder = match backend {
                            DbBackend::Sqlite => format!("?{}", next),
                            _ => format!(":b{}", next),
                        };
                        *next += 1;
                        out.push_str(&placeholder);
                        binds.push((placeholder, v.to_sql_value()));
                        continue;
                    }
                }
                out.push_str(&word);
                continue;
            }
            out.push(c);
            i += 1;
        }
        Ok((out, binds))
    }

    fn run_query(&mut self, sql: &str, session: &mut FormSession) -> Result<Result<QueryResult, RaisedException>, RuntimeError> {
        let bound = match self.prepare_sql(sql, session)? {
            Ok(b) => b,
            Err(ex) => return Ok(Err(ex)),
        };
        let driver = session.database_driver.clone().expect("checked in prepare_sql");
        session.trace_log.push(format!("SQL: {}", bound.sql));
        match block_on_future(driver.query(&bound)) {
            Ok(r) => Ok(Ok(r)),
            Err(e) => Ok(Err(RaisedException::from_db_error(&e.to_string()))),
        }
    }

    fn run_cursor_query(&mut self, decl: &CursorDecl, args: &[Expr], session: &mut FormSession) -> Result<Result<QueryResult, RaisedException>, RuntimeError> {
        // Cursor parameters are visible to the query as variables.
        let mut values = Vec::new();
        for a in args {
            match self.eval(a, session)? {
                Ok(v) => values.push(v),
                Err(ex) => return Ok(Err(ex)),
            }
        }
        self.frames.push(Frame::default());
        for (i, p) in decl.params.iter().enumerate() {
            let v = match values.get(i) {
                Some(v) => v.clone(),
                None => match &p.default_value {
                    Some(e) => match self.eval(e, session)? {
                        Ok(v) => v,
                        Err(ex) => {
                            self.frames.pop();
                            return Ok(Err(ex));
                        }
                    },
                    None => OracleValue::Null,
                },
            };
            self.declare(&p.name, v);
        }
        let result = self.run_query(&decl.query, session);
        self.frames.pop();
        result
    }

    fn exec_sql(&mut self, sql: &SqlStatement, session: &mut FormSession) -> ExecResult {
        match sql {
            SqlStatement::SelectInto { into_targets, raw_sql, .. } => {
                let result = match self.run_query(raw_sql, session)? {
                    Ok(r) => r,
                    Err(ex) => {
                        self.sql_rowcount = Some(0);
                        return Ok(Flow::Raise(ex));
                    }
                };
                match result.records.len() {
                    0 => {
                        self.sql_rowcount = Some(0);
                        Ok(Flow::Raise(RaisedException::new("NO_DATA_FOUND", 100, "ORA-01403: no data found")))
                    }
                    1 => {
                        self.sql_rowcount = Some(1);
                        let columns: Vec<String> = result.columns.iter().map(|c| upper(c)).collect();
                        let mut seen = std::collections::HashSet::new();
                        if let Some(dup) = columns.iter().find(|c| !seen.insert((*c).clone())) {
                            return Err(unsupported(format!(
                                "SELECT INTO returns duplicate column name {}; give the select-list items distinct aliases",
                                dup
                            )));
                        }
                        let row = result.records[0].values.clone();
                        self.assign_row(into_targets, &columns, &row, session)
                    }
                    n => {
                        self.sql_rowcount = Some(n as i64);
                        Ok(Flow::Raise(RaisedException::new(
                            "TOO_MANY_ROWS",
                            -1422,
                            "ORA-01422: exact fetch returns more than requested number of rows",
                        )))
                    }
                }
            }
            SqlStatement::Dml { kind, raw_sql } => match kind {
                DmlKind::Commit => {
                    // COMMIT in a Forms trigger means COMMIT_FORM.
                    self.run_builtin("COMMIT_FORM", &[], session).map(|r| match r {
                        Ok(_) => Flow::Normal,
                        Err(ex) => Flow::Raise(ex),
                    })
                }
                DmlKind::Rollback => {
                    if let Some(driver) = session.database_driver.clone() {
                        if let Err(e) = block_on_future(driver.rollback()) {
                            return Ok(Flow::Raise(RaisedException::from_db_error(&e.to_string())));
                        }
                    }
                    session.clear_form();
                    Ok(Flow::Normal)
                }
                _ => {
                    if session.read_only {
                        return Ok(Flow::Raise(RaisedException::new(
                            "READ_ONLY_SESSION",
                            -20001,
                            format!("DML blocked: this preview session is read-only ({})", raw_sql),
                        )));
                    }
                    let bound = match self.prepare_sql(raw_sql, session)? {
                        Ok(b) => b,
                        Err(ex) => return Ok(Flow::Raise(ex)),
                    };
                    let driver = session.database_driver.clone().expect("checked in prepare_sql");
                    session.trace_log.push(format!("SQL_DML: {}", bound.sql));
                    match block_on_future(driver.execute(&bound)) {
                        Ok(n) => {
                            self.sql_rowcount = Some(n as i64);
                            Ok(Flow::Normal)
                        }
                        Err(e) => {
                            self.sql_rowcount = Some(0);
                            Ok(Flow::Raise(RaisedException::from_db_error(&e.to_string())))
                        }
                    }
                }
            },
        }
    }

    // ------------------------------------------------------------------
    // Calls
    // ------------------------------------------------------------------

    /// Calls a procedure or function: form/library program units first, then Forms
    /// built-ins, then standard SQL functions, then the database (Oracle only).
    /// Returns `Ok(Ok(return_value))`, or `Ok(Err(exception))` if one propagated.
    fn call(&mut self, name: &str, args: &[Expr], session: &mut FormSession, want_value: bool) -> Result<Result<Option<OracleValue>, RaisedException>, RuntimeError> {
        let programs = session.programs.clone();
        if let Some(routine) = programs.resolve(name, self.current_package().as_deref()) {
            if want_value && !routine.is_function {
                return Err(unsupported(format!("PLS-00222: {} is a procedure, not a function", name)));
            }
            return self.call_routine(&routine, args, session);
        }

        // Everything else takes evaluated IN arguments.
        let mut values = Vec::with_capacity(args.len());
        for a in args {
            if let Expr::NamedArg { name: arg, .. } = a {
                return Err(unsupported(format!("named argument {} => is only supported for program units", arg)));
            }
            match self.eval(a, session)? {
                Ok(v) => values.push(v),
                Err(ex) => return Ok(Err(ex)),
            }
        }

        match self.run_builtin(name, &values, session)? {
            Ok(Some(v)) => return Ok(Ok(Some(v))),
            Ok(None) => {}
            Err(ex) => return Ok(Err(ex)),
        }

        let up = upper(name);
        match up.as_str() {
            "SQLCODE" => return Ok(Ok(Some(OracleValue::int(self.handling.last().map(|e| e.code).unwrap_or(0))))),
            "SQLERRM" => {
                let msg = match values.first().and_then(|v| v.as_i64()) {
                    Some(code) => format!("ORA-{:05}", code.abs()),
                    None => self
                        .handling
                        .last()
                        .map(|e| e.message.clone())
                        .unwrap_or_else(|| "ORA-0000: normal, successful completion".into()),
                };
                return Ok(Ok(Some(OracleValue::varchar2(msg))));
            }
            "USER" => return Ok(Ok(Some(OracleValue::varchar2(session.db_username.clone().unwrap_or_default().to_ascii_uppercase())))),
            "RAISE_APPLICATION_ERROR" => {
                let code = values.first().and_then(|v| v.as_i64()).unwrap_or(-20000);
                let msg = values.get(1).map(|v| v.to_string_lossy()).unwrap_or_default();
                return Ok(Err(RaisedException::new("APPLICATION_ERROR", code, format!("ORA{}: {}", code, msg))));
            }
            _ => {}
        }
        if let Some(res) = stdfuncs::call(&up, &values) {
            return match res {
                Ok(v) => Ok(Ok(Some(v))),
                Err(e) => Ok(Err(RaisedException::from_value_error(e))),
            };
        }

        self.call_database(&up, &values, session, want_value)
    }

    /// Runs a Forms built-in. `Ok(None)` if `name` is not a built-in.
    fn run_builtin(&mut self, name: &str, values: &[OracleValue], session: &mut FormSession) -> Result<Result<Option<OracleValue>, RaisedException>, RuntimeError> {
        match builtins::dispatch(name, values, session) {
            None => Ok(Ok(None)),
            Some(BuiltinResult::Value(v)) => {
                session.last_builtin_failed = false;
                Ok(Ok(Some(v)))
            }
            Some(BuiltinResult::Void) => {
                session.last_builtin_failed = false;
                Ok(Ok(Some(OracleValue::Null)))
            }
            Some(BuiltinResult::Failure(msg)) => {
                // Forms: the built-in fails, FORM_FAILURE becomes TRUE, and the trigger continues.
                session.last_builtin_failed = true;
                session.last_error_text = Some(msg);
                Ok(Ok(Some(OracleValue::Null)))
            }
            Some(BuiltinResult::Unsupported(why)) => Err(unsupported(format!("built-in {}", why))),
        }
    }

    /// Delegates an unknown subprogram to the database as a stored procedure/function.
    fn call_database(&mut self, name: &str, values: &[OracleValue], session: &mut FormSession, want_value: bool) -> Result<Result<Option<OracleValue>, RaisedException>, RuntimeError> {
        let Some(driver) = session.database_driver.clone() else {
            return Err(unsupported(format!("PLS-00201: identifier '{}' must be declared (not a program unit, library unit or built-in, and no database is attached)", name)));
        };
        if !matches!(driver.backend(), DbBackend::Oracle | DbBackend::OracleThin) {
            return Err(unsupported(format!("PLS-00201: identifier '{}' must be declared", name)));
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '#')) {
            return Err(unsupported(format!("invalid subprogram name {}", name)));
        }
        if want_value && values.is_empty() && !name.contains('.') {
            // A bare unknown identifier in an expression is almost always an undeclared
            // variable, not a parameterless stored function: report it.
            return Err(unsupported(format!("PLS-00201: identifier '{}' must be declared", name)));
        }
        let placeholders: Vec<String> = (1..=values.len()).map(|i| format!(":b{}", i)).collect();
        let binds = placeholders.iter().cloned().zip(values.iter().map(|v| v.to_sql_value())).collect();
        if want_value {
            let sql = format!("SELECT {}({}) AS RESULT FROM DUAL", name, placeholders.join(", "));
            session.trace_log.push(format!("DB_FUNCTION: {}", sql));
            return match block_on_future(driver.query(&BoundSql { sql, binds })) {
                Ok(r) => Ok(Ok(Some(
                    r.records.first().and_then(|rec| rec.values.get("RESULT")).map(OracleValue::from_json).unwrap_or(OracleValue::Null),
                ))),
                Err(e) => Ok(Err(RaisedException::from_db_error(&e.to_string()))),
            };
        }
        if session.read_only {
            return Ok(Err(RaisedException::new(
                "READ_ONLY_SESSION",
                -20001,
                format!("Call to database procedure {} blocked: this preview session is read-only", name),
            )));
        }
        let sql = format!("BEGIN {}({}); END;", name, placeholders.join(", "));
        session.trace_log.push(format!("DB_PROCEDURE: {}", sql));
        match block_on_future(driver.execute(&BoundSql { sql, binds })) {
            Ok(_) => Ok(Ok(None)),
            Err(e) => Ok(Err(RaisedException::from_db_error(&e.to_string()))),
        }
    }

    fn call_routine(&mut self, routine: &Arc<Routine>, args: &[Expr], session: &mut FormSession) -> Result<Result<Option<OracleValue>, RaisedException>, RuntimeError> {
        let depth = self.frames.iter().filter(|f| f.boundary).count();
        if depth >= self.max_depth {
            return Err(RuntimeError::RecursionLimitExceeded);
        }
        if let Some(pkg) = &routine.package {
            if let Some(ex) = self.ensure_package(pkg, session)? {
                return Ok(Err(ex));
            }
        }

        // Match actual arguments to formal parameters (positional, then named).
        let mut bound: Vec<Option<&Expr>> = vec![None; routine.params.len()];
        let mut positional = 0;
        for a in args {
            match a {
                Expr::NamedArg { name, value } => {
                    let idx = routine
                        .params
                        .iter()
                        .position(|p| p.name.eq_ignore_ascii_case(name))
                        .ok_or_else(|| unsupported(format!("PLS-00306: {} has no parameter {}", routine.name, name)))?;
                    bound[idx] = Some(value);
                }
                other => {
                    if positional >= bound.len() {
                        return Err(unsupported(format!("PLS-00306: wrong number of arguments in call to {}", routine.name)));
                    }
                    bound[positional] = Some(other);
                    positional += 1;
                }
            }
        }

        let mut frame = Frame { boundary: true, package: routine.package.clone(), ..Default::default() };
        let mut out_targets: Vec<(String, &Expr)> = Vec::new();
        for (p, arg) in routine.params.iter().zip(&bound) {
            let value = match (p.mode, arg) {
                (ParamMode::Out, Some(target)) => {
                    out_targets.push((upper(&p.name), target));
                    OracleValue::Null
                }
                (ParamMode::InOut, Some(target)) => {
                    out_targets.push((upper(&p.name), target));
                    match self.eval(target, session)? {
                        Ok(v) => v,
                        Err(ex) => return Ok(Err(ex)),
                    }
                }
                (ParamMode::In, Some(e)) => match self.eval(e, session)? {
                    Ok(v) => v,
                    Err(ex) => return Ok(Err(ex)),
                },
                (_, None) => match &p.default_value {
                    Some(d) => match self.eval(d, session)? {
                        Ok(v) => v,
                        Err(ex) => return Ok(Err(ex)),
                    },
                    None if p.mode == ParamMode::In => {
                        return Err(unsupported(format!("PLS-00306: missing argument {} in call to {}", p.name, routine.name)));
                    }
                    None => return Err(unsupported(format!("PLS-00363: OUT argument {} of {} needs a target", p.name, routine.name))),
                },
            };
            let ty = match &p.data_type {
                oracle_forms_plsql::ast::DataType::Primitive(t) | oracle_forms_plsql::ast::DataType::Custom(t) => Some(t.clone()),
                _ => None,
            };
            let value = match &ty {
                Some(t) => value.coerce_to(t),
                None => value,
            };
            if let Some(t) = ty {
                frame.types.insert(upper(&p.name), t);
            }
            frame.vars.insert(upper(&p.name), value);
        }
        validate_params(&routine.params)?;

        self.frames.push(frame);
        let flow = (|| -> ExecResult {
            if let Some(ex) = self.declare_all(&routine.declarations, session)? {
                return Ok(Flow::Raise(ex));
            }
            let flow = self.exec_statements(&routine.body, session)?;
            self.handle(flow, &routine.exception_handlers, session)
        })();
        let frame = self.frames.pop().expect("pushed above");
        let flow = flow?;

        let ret = match flow {
            Flow::Raise(ex) => return Ok(Err(ex)),
            Flow::Return(v) => v,
            Flow::Normal | Flow::Exit => {
                if routine.is_function {
                    return Ok(Err(RaisedException::new("PROGRAM_ERROR", -6503, format!("ORA-06503: PL/SQL: Function {} returned without value", routine.name))));
                }
                None
            }
        };

        // Copy OUT / IN OUT values back to the caller's targets.
        for (param, target) in out_targets {
            let value = frame.vars.get(&param).cloned().unwrap_or(OracleValue::Null);
            if let Flow::Raise(ex) = self.assign(target, value, session)? {
                return Ok(Err(ex));
            }
        }
        Ok(Ok(ret))
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn eval_many(&mut self, exprs: &[Expr], session: &mut FormSession) -> Result<Result<Vec<OracleValue>, RaisedException>, RuntimeError> {
        let mut out = Vec::with_capacity(exprs.len());
        for e in exprs {
            match self.eval(e, session)? {
                Ok(v) => out.push(v),
                Err(ex) => return Ok(Err(ex)),
            }
        }
        Ok(Ok(out))
    }

    /// Evaluates an expression. `Ok(Err(exception))` means a PL/SQL exception was raised.
    pub(crate) fn eval(&mut self, expr: &Expr, session: &mut FormSession) -> Result<Result<OracleValue, RaisedException>, RuntimeError> {
        macro_rules! try_ex {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(ex) => return Ok(Err(ex)),
                }
            };
        }
        Ok(Ok(match expr {
            Expr::Literal(lit) => OracleValue::from_literal(lit),

            Expr::Bind(name) => {
                let up = upper(name.trim_start_matches(':'));
                if let Some(seq) = up.strip_prefix("SEQUENCE.") {
                    return self.next_sequence_value(seq, session);
                }
                session.get_bind_value(&up)
            }

            Expr::Identifier(name) => {
                let up = upper(name);
                match up.as_str() {
                    "ALERT_BUTTON1" => return Ok(Ok(OracleValue::int(ALERT_BUTTON1))),
                    "ALERT_BUTTON2" => return Ok(Ok(OracleValue::int(ALERT_BUTTON2))),
                    "ALERT_BUTTON3" => return Ok(Ok(OracleValue::int(ALERT_BUTTON3))),
                    "PROPERTY_TRUE" | "PROPERTY_ON" => return Ok(Ok(OracleValue::int(PROPERTY_TRUE))),
                    "PROPERTY_FALSE" | "PROPERTY_OFF" => return Ok(Ok(OracleValue::int(PROPERTY_FALSE))),
                    _ => {}
                }
                if let Some(v) = self.lookup_var(&up) {
                    return Ok(Ok(v));
                }
                if let Some(key) = self.package_var_key(&up, session) {
                    if let Some(v) = session.package_vars.get(&key) {
                        return Ok(Ok(v.clone()));
                    }
                }
                if builtins::is_forms_constant(&up) {
                    return Ok(Ok(OracleValue::varchar2(up)));
                }
                // A parameterless function call: `v := my_func;`, `SYSDATE`, `FORM_SUCCESS`.
                match self.call(&up, &[], session, true)? {
                    Ok(v) => v.unwrap_or(OracleValue::Null),
                    Err(ex) => return Ok(Err(ex)),
                }
            }

            Expr::MemberAccess { .. } => {
                let name = dotted(expr).unwrap_or_default();
                if let Some(v) = self.lookup_var(&name) {
                    return Ok(Ok(v));
                }
                if let Some(key) = self.package_var_key(&name, session) {
                    let pkg = key.split('.').next().unwrap_or_default().to_string();
                    if let Some(ex) = self.ensure_package(&pkg, session)? {
                        return Ok(Err(ex));
                    }
                    if let Some(v) = session.package_vars.get(&key) {
                        return Ok(Ok(v.clone()));
                    }
                }
                match self.call(&name, &[], session, true)? {
                    Ok(v) => v.unwrap_or(OracleValue::Null),
                    Err(ex) => return Ok(Err(ex)),
                }
            }

            Expr::Parenthesized(inner) => try_ex!(self.eval(inner, session)?),

            Expr::BinaryOp { left, op, right } => {
                let l = try_ex!(self.eval(left, session)?);
                // Short-circuit AND / OR like PL/SQL.
                use oracle_forms_plsql::ast::BinaryOp;
                if *op == BinaryOp::And && l == OracleValue::boolean(false) {
                    return Ok(Ok(l));
                }
                if *op == BinaryOp::Or && l == OracleValue::boolean(true) {
                    return Ok(Ok(l));
                }
                let r = try_ex!(self.eval(right, session)?);
                try_ex!(l.eval_binary_op(*op, &r).map_err(RaisedException::from_value_error))
            }

            Expr::UnaryOp { op, expr } => {
                let v = try_ex!(self.eval(expr, session)?);
                try_ex!(v.eval_unary_op(*op).map_err(RaisedException::from_value_error))
            }

            Expr::FunctionCall { name, args } => match self.call(name, args, session, true)? {
                Ok(v) => v.unwrap_or(OracleValue::Null),
                Err(ex) => return Ok(Err(ex)),
            },

            Expr::InList { expr, list, negated } => {
                let v = try_ex!(self.eval(expr, session)?);
                let items = try_ex!(self.eval_many(list, session)?);
                let mut result = Some(false);
                for item in &items {
                    match try_ex!(v.eval_binary_op(oracle_forms_plsql::ast::BinaryOp::Equal, item).map_err(RaisedException::from_value_error)) {
                        OracleValue::Boolean(Some(true)) => {
                            result = Some(true);
                            break;
                        }
                        OracleValue::Boolean(None) => result = None,
                        _ => {}
                    }
                }
                let result = if *negated { result.map(|b| !b) } else { result };
                OracleValue::Boolean(result)
            }

            Expr::Between { expr, low, high, negated } => {
                use oracle_forms_plsql::ast::BinaryOp;
                let v = try_ex!(self.eval(expr, session)?);
                let lo = try_ex!(self.eval(low, session)?);
                let hi = try_ex!(self.eval(high, session)?);
                let ge = try_ex!(v.eval_binary_op(BinaryOp::GreaterThanOrEqual, &lo).map_err(RaisedException::from_value_error));
                let le = try_ex!(v.eval_binary_op(BinaryOp::LessThanOrEqual, &hi).map_err(RaisedException::from_value_error));
                let both = try_ex!(ge.eval_binary_op(BinaryOp::And, &le).map_err(RaisedException::from_value_error));
                if *negated {
                    try_ex!(both.eval_unary_op(oracle_forms_plsql::ast::UnaryOp::Not).map_err(RaisedException::from_value_error))
                } else {
                    both
                }
            }

            Expr::Attribute { object, attr } => {
                if object == "SQL" {
                    match attr.as_str() {
                        "ROWCOUNT" => self.sql_rowcount.map(OracleValue::int).unwrap_or(OracleValue::Null),
                        "FOUND" => OracleValue::Boolean(self.sql_rowcount.map(|n| n > 0)),
                        "NOTFOUND" => OracleValue::Boolean(self.sql_rowcount.map(|n| n == 0)),
                        "ISOPEN" => OracleValue::boolean(false),
                        other => return Err(unsupported(format!("SQL%{}", other))),
                    }
                } else {
                    let state = self.open_cursor_mut(object).cloned();
                    match (attr.as_str(), state) {
                        ("ISOPEN", s) => OracleValue::boolean(s.is_some()),
                        ("FOUND", Some(s)) => OracleValue::Boolean(s.found),
                        ("NOTFOUND", Some(s)) => OracleValue::Boolean(s.found.map(|b| !b)),
                        ("ROWCOUNT", Some(s)) => OracleValue::int(s.position as i64),
                        (_, None) => return Ok(Err(RaisedException::new("INVALID_CURSOR", -1001, "ORA-01001: invalid cursor"))),
                        (other, _) => return Err(unsupported(format!("{}%{}", object, other))),
                    }
                }
            }

            Expr::Case { selector, whens, else_expr } => {
                let sel = match selector {
                    Some(s) => Some(try_ex!(self.eval(s, session)?)),
                    None => None,
                };
                for (cond, val) in whens {
                    let c = try_ex!(self.eval(cond, session)?);
                    let hit = match &sel {
                        Some(s) => try_ex!(s.eval_binary_op(oracle_forms_plsql::ast::BinaryOp::Equal, &c).map_err(RaisedException::from_value_error)).is_truthy(),
                        None => c.is_truthy(),
                    };
                    if hit {
                        return self.eval(val, session);
                    }
                }
                match else_expr {
                    Some(e) => try_ex!(self.eval(e, session)?),
                    None => OracleValue::Null,
                }
            }

            Expr::NamedArg { name, .. } => return Err(unsupported(format!("named argument {} outside a call", name))),
        }))
    }

    /// `:SEQUENCE.NAME.NEXTVAL`: from the database when one is attached (Oracle), else a
    /// local counter (recorded in the trace so it is never mistaken for a real key).
    fn next_sequence_value(&mut self, seq: &str, session: &mut FormSession) -> Result<Result<OracleValue, RaisedException>, RuntimeError> {
        let name = seq.split('.').next().unwrap_or(seq).to_string();
        if let Some(driver) = session.database_driver.clone() {
            if matches!(driver.backend(), DbBackend::Oracle | DbBackend::OracleThin) {
                if !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '#')) {
                    return Err(unsupported(format!("invalid sequence name {}", name)));
                }
                let sql = format!("SELECT {}.NEXTVAL AS NEXTVAL FROM DUAL", name);
                return Ok(match block_on_future(driver.query(&BoundSql::new(&sql))) {
                    Ok(r) => Ok(r.records.first().and_then(|rec| rec.values.get("NEXTVAL")).map(OracleValue::from_json).unwrap_or(OracleValue::Null)),
                    Err(e) => Err(RaisedException::from_db_error(&e.to_string())),
                });
            }
        }
        session.trace_log.push(format!("LOCAL_SEQUENCE: {} (no Oracle database attached)", name));
        Ok(Ok(session.get_bind_value(&format!("SEQUENCE.{}.NEXTVAL", name))))
    }
}

fn validate_params(params: &[ParameterDecl]) -> Result<(), RuntimeError> {
    let mut seen = std::collections::HashSet::new();
    for p in params {
        if !seen.insert(upper(&p.name)) {
            return Err(unsupported(format!("duplicate parameter {}", p.name)));
        }
    }
    Ok(())
}
