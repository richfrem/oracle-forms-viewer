use crate::builtins::{
    ALERT_BUTTON1, ALERT_BUTTON2, ALERT_BUTTON3, BuiltinRegistry, BuiltinResult, BuiltinSuspension,
    PROPERTY_FALSE, PROPERTY_TRUE,
};
use crate::session::{FormSession, RuntimeError};
use crate::value::OracleValue;
use oracle_forms_plsql::ast::{
    BlockStmt, Declaration, Expr, PlsqlUnit, SqlStatement, Statement,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    Message {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InteractionResponse {
    AlertButton(i64),
    Acknowledge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackFrame {
    pub name: String,
    pub variables: HashMap<String, OracleValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuspensionContext {
    pub interaction: InteractionRequest,
    pub call_stack: Vec<StackFrame>,
    pub remaining_statements: Vec<Statement>,
    pub resume_target_assignment: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStatus {
    Success(Option<OracleValue>),
    Suspended(Box<SuspensionContext>),
    TriggerFailure(String),
    UnhandledException(String),
}

impl ExecutionStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, ExecutionStatus::Success(_))
    }
}

pub struct Interpreter {
    pub step_count: usize,
    pub max_steps: usize,
    pub call_stack: Vec<StackFrame>,
    pub max_depth: usize,
    pub interactive: bool,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self {
            step_count: 0,
            max_steps: 100_000,
            call_stack: vec![StackFrame {
                name: "GLOBAL".to_string(),
                variables: HashMap::new(),
            }],
            max_depth: 64,
            interactive: false,
        }
    }
}

impl Interpreter {
    pub fn new(interactive: bool) -> Self {
        Self {
            interactive,
            ..Default::default()
        }
    }

    /// Evaluates an AST PlsqlUnit (e.g. from a Trigger or Program Unit).
    pub fn execute_unit(
        &mut self,
        unit: &PlsqlUnit,
        session: &mut FormSession,
    ) -> Result<ExecutionStatus, RuntimeError> {
        match unit {
            PlsqlUnit::AnonymousBlock(b) => self.execute_block_stmt(b, session),
            PlsqlUnit::StatementList(stmts) => self.execute_statements(stmts, session),
            PlsqlUnit::Procedure(p) => {
                let block = BlockStmt {
                    declarations: p.declarations.clone(),
                    body: p.body.clone(),
                    exception_handlers: p.exception_handlers.clone(),
                };
                self.execute_block_stmt(&block, session)
            }
            PlsqlUnit::Function(f) => {
                let block = BlockStmt {
                    declarations: f.declarations.clone(),
                    body: f.body.clone(),
                    exception_handlers: f.exception_handlers.clone(),
                };
                self.execute_block_stmt(&block, session)
            }
            _ => Ok(ExecutionStatus::Success(None)),
        }
    }

    pub fn execute_block_stmt(
        &mut self,
        block: &BlockStmt,
        session: &mut FormSession,
    ) -> Result<ExecutionStatus, RuntimeError> {
        if self.call_stack.len() >= self.max_depth {
            return Err(RuntimeError::RecursionLimitExceeded);
        }

        let mut frame = StackFrame {
            name: "BLOCK".to_string(),
            variables: HashMap::new(),
        };

        // Initialize local variable declarations with default values if any
        for decl in &block.declarations {
            if let Declaration::Variable(v) = decl {
                let initial = if let Some(def_expr) = &v.default_value {
                    self.eval_expr(def_expr, session)?
                } else {
                    OracleValue::Null
                };
                frame.variables.insert(v.name.to_ascii_uppercase(), initial);
            }
        }

        self.call_stack.push(frame);
        let status = self.execute_statements(&block.body, session);

        match status {
            Ok(ExecutionStatus::UnhandledException(ref ex_name)) => {
                // Check if caught by exception handler
                for handler in &block.exception_handlers {
                    if handler.exceptions.iter().any(|e| {
                        e.eq_ignore_ascii_case(ex_name) || e.eq_ignore_ascii_case("OTHERS")
                    }) {
                        let handler_status = self.execute_statements(&handler.body, session);
                        self.call_stack.pop();
                        return handler_status;
                    }
                }
                self.call_stack.pop();
                Ok(ExecutionStatus::UnhandledException(ex_name.clone()))
            }
            Ok(ExecutionStatus::Suspended(ctx)) => {
                // If execution is suspended, keep call stack in context
                self.call_stack.pop();
                Ok(ExecutionStatus::Suspended(ctx))
            }
            other => {
                self.call_stack.pop();
                other
            }
        }
    }

    pub fn execute_statements(
        &mut self,
        stmts: &[Statement],
        session: &mut FormSession,
    ) -> Result<ExecutionStatus, RuntimeError> {
        for (idx, stmt) in stmts.iter().enumerate() {
            self.step_count += 1;
            if self.step_count > self.max_steps {
                return Err(RuntimeError::StepBudgetExceeded);
            }

            match self.execute_statement(stmt, session)? {
                ExecutionStatus::Success(None) => continue,
                ExecutionStatus::Success(Some(val)) => {
                    return Ok(ExecutionStatus::Success(Some(val)));
                }
                ExecutionStatus::Suspended(mut ctx) => {
                    // Prepend the remainder of the statements to the suspension context
                    let remaining = stmts[idx + 1..].to_vec();
                    ctx.remaining_statements.splice(0..0, remaining);
                    return Ok(ExecutionStatus::Suspended(ctx));
                }
                failure @ (ExecutionStatus::TriggerFailure(_)
                | ExecutionStatus::UnhandledException(_)) => {
                    return Ok(failure);
                }
            }
        }

        Ok(ExecutionStatus::Success(None))
    }

    pub fn execute_statement(
        &mut self,
        stmt: &Statement,
        session: &mut FormSession,
    ) -> Result<ExecutionStatus, RuntimeError> {
        match stmt {
            Statement::Assignment { target, value } => {
                // Check if value is a function call that suspends (e.g. SHOW_ALERT)
                if let Expr::FunctionCall { name, args } = value {
                    let mut eval_args = Vec::new();
                    for a in args {
                        eval_args.push(self.eval_expr(a, session)?);
                    }

                    match BuiltinRegistry::dispatch(name, &eval_args, session, self.interactive) {
                        BuiltinResult::Suspend(BuiltinSuspension::ShowAlert {
                            alert_name,
                            message,
                            button1,
                            button2,
                            button3,
                            default_button,
                        }) => {
                            let ctx = SuspensionContext {
                                interaction: InteractionRequest::ShowAlert {
                                    alert_name,
                                    message,
                                    button1,
                                    button2,
                                    button3,
                                    default_button,
                                },
                                call_stack: self.call_stack.clone(),
                                remaining_statements: Vec::new(),
                                resume_target_assignment: Some(target.clone()),
                            };
                            return Ok(ExecutionStatus::Suspended(Box::new(ctx)));
                        }
                        BuiltinResult::Value(val) => {
                            self.assign_target(target, val, session)?;
                            return Ok(ExecutionStatus::Success(None));
                        }
                        BuiltinResult::Failure(msg) => {
                            return Ok(ExecutionStatus::TriggerFailure(msg));
                        }
                        BuiltinResult::Void => {
                            self.assign_target(target, OracleValue::Null, session)?;
                            return Ok(ExecutionStatus::Success(None));
                        }
                    }
                }

                let val = self.eval_expr(value, session)?;
                self.assign_target(target, val, session)?;
                Ok(ExecutionStatus::Success(None))
            }

            Statement::Call { target, args } => {
                let mut eval_args = Vec::new();
                for a in args {
                    eval_args.push(self.eval_expr(a, session)?);
                }

                match BuiltinRegistry::dispatch(target, &eval_args, session, self.interactive) {
                    BuiltinResult::Suspend(BuiltinSuspension::ShowAlert {
                        alert_name,
                        message,
                        button1,
                        button2,
                        button3,
                        default_button,
                    }) => {
                        let ctx = SuspensionContext {
                            interaction: InteractionRequest::ShowAlert {
                                alert_name,
                                message,
                                button1,
                                button2,
                                button3,
                                default_button,
                            },
                            call_stack: self.call_stack.clone(),
                            remaining_statements: Vec::new(),
                            resume_target_assignment: None,
                        };
                        Ok(ExecutionStatus::Suspended(Box::new(ctx)))
                    }
                    BuiltinResult::Failure(msg) => Ok(ExecutionStatus::TriggerFailure(msg)),
                    _ => Ok(ExecutionStatus::Success(None)),
                }
            }

            Statement::If {
                condition,
                then_branch,
                elsifs,
                else_branch,
            } => {
                let cond_val = self.eval_expr(condition, session)?;
                if cond_val.is_truthy() {
                    return self.execute_statements(then_branch, session);
                }

                for (elsif_cond, elsif_branch) in elsifs {
                    let c = self.eval_expr(elsif_cond, session)?;
                    if c.is_truthy() {
                        return self.execute_statements(elsif_branch, session);
                    }
                }

                if let Some(else_stmts) = else_branch {
                    return self.execute_statements(else_stmts, session);
                }

                Ok(ExecutionStatus::Success(None))
            }

            Statement::Loop { body } => {
                loop {
                    self.step_count += 1;
                    if self.step_count > self.max_steps {
                        return Err(RuntimeError::StepBudgetExceeded);
                    }
                    let res = self.execute_statements(body, session)?;
                    match res {
                        ExecutionStatus::Success(None) => continue,
                        other => return Ok(other),
                    }
                }
            }

            Statement::WhileLoop { condition, body } => {
                while self.eval_expr(condition, session)?.is_truthy() {
                    self.step_count += 1;
                    if self.step_count > self.max_steps {
                        return Err(RuntimeError::StepBudgetExceeded);
                    }
                    let res = self.execute_statements(body, session)?;
                    match res {
                        ExecutionStatus::Success(None) => continue,
                        other => return Ok(other),
                    }
                }
                Ok(ExecutionStatus::Success(None))
            }

            Statement::ForLoop {
                var_name,
                reverse,
                lower,
                upper,
                body,
            } => {
                let l_val = self
                    .eval_expr(lower, session)?
                    .as_number()
                    .unwrap_or(0.0) as i64;
                let u_val = self
                    .eval_expr(upper, session)?
                    .as_number()
                    .unwrap_or(0.0) as i64;

                let range: Box<dyn Iterator<Item = i64>> = if *reverse {
                    Box::new((l_val..=u_val).rev())
                } else {
                    Box::new(l_val..=u_val)
                };

                for i in range {
                    self.step_count += 1;
                    if self.step_count > self.max_steps {
                        return Err(RuntimeError::StepBudgetExceeded);
                    }
                    if let Some(frame) = self.call_stack.last_mut() {
                        frame
                            .variables
                            .insert(var_name.to_ascii_uppercase(), OracleValue::Number(i as f64));
                    }
                    let res = self.execute_statements(body, session)?;
                    match res {
                        ExecutionStatus::Success(None) => continue,
                        other => return Ok(other),
                    }
                }
                Ok(ExecutionStatus::Success(None))
            }

            Statement::Return(opt_expr) => {
                if let Some(e) = opt_expr {
                    let val = self.eval_expr(e, session)?;
                    Ok(ExecutionStatus::Success(Some(val)))
                } else {
                    Ok(ExecutionStatus::Success(Some(OracleValue::Null)))
                }
            }

            Statement::Raise(opt_name) => {
                let ex = opt_name
                    .as_deref()
                    .unwrap_or("FORM_TRIGGER_FAILURE")
                    .to_ascii_uppercase();
                if ex == "FORM_TRIGGER_FAILURE" {
                    Ok(ExecutionStatus::TriggerFailure("FORM_TRIGGER_FAILURE".to_string()))
                } else {
                    Ok(ExecutionStatus::UnhandledException(ex))
                }
            }

            Statement::Block(b) => self.execute_block_stmt(b, session),

            Statement::Sql(sql_stmt) => self.execute_sql_stmt(sql_stmt, session),

            Statement::Null => Ok(ExecutionStatus::Success(None)),

            Statement::Opaque { raw, .. } => {
                session.trace_log.push(format!("OPAQUE_STMT: {}", raw));
                Ok(ExecutionStatus::Success(None))
            }
        }
    }

    /// Evaluates an embedded SQL statement (e.g. SELECT ... INTO ...).
    fn execute_sql_stmt(
        &mut self,
        sql_stmt: &SqlStatement,
        session: &mut FormSession,
    ) -> Result<ExecutionStatus, RuntimeError> {
        match sql_stmt {
            SqlStatement::SelectInto {
                select_clause: _,
                into_targets,
                from_clause: _,
                where_clause: _,
                raw_sql,
            } => {
                session.trace_log.push(format!("SQL_SELECT_INTO: {}", raw_sql));

                // If a database driver is attached to the session, execute it
                if let Some(driver) = &session.database_driver {
                    let sql = raw_sql.clone();
                    let mut binds = Vec::new();

                    // Substitute :BLOCK.ITEM binds
                    let bind_re = regex::Regex::new(r":([A-Z0-9_$.]+)").unwrap();
                    for cap in bind_re.captures_iter(raw_sql) {
                        let _bind_full = cap.get(0).unwrap().as_str();
                        let bind_target = cap.get(1).unwrap().as_str();
                        let val = session.get_bind_value(bind_target);
                        binds.push((bind_target.to_string(), val.to_sql_value()));
                    }

                    let bound = oracle_forms_db::model::BoundSql { sql, binds };
                    // Execute query via tokio block_in_place or synchronous handle
                    let query_fut = driver.query(&bound);
                    let result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(query_fut)
                    });

                    if let Ok(res) = result {
                        if let Some(record) = res.records.first() {
                            for (idx, target) in into_targets.iter().enumerate() {
                                if let Some((_, val)) = record.values.iter().nth(idx) {
                                    let oval = OracleValue::from_json(val);
                                    self.assign_target(target, oval, session)?;
                                }
                            }
                        }
                    }
                } else {
                    // Fallback in-memory evaluator for tests
                    for target in into_targets {
                        // Default to 1 for count(*) or mock values
                        self.assign_target(target, OracleValue::Number(1.0), session)?;
                    }
                }

                Ok(ExecutionStatus::Success(None))
            }
            SqlStatement::Dml { kind, raw_sql: _ } => {
                session.trace_log.push(format!("SQL_DML: {:?}", kind));
                Ok(ExecutionStatus::Success(None))
            }
        }
    }

    /// Resumes a suspended execution given the user's interaction response.
    pub fn resume(
        &mut self,
        session: &mut FormSession,
        ctx: SuspensionContext,
        response: InteractionResponse,
    ) -> Result<ExecutionStatus, RuntimeError> {
        self.call_stack = ctx.call_stack;

        // If the suspension was inside an assignment (e.g. v_button := SHOW_ALERT(...))
        if let Some(target) = ctx.resume_target_assignment {
            let res_val = match response {
                InteractionResponse::AlertButton(btn) => OracleValue::Number(btn as f64),
                InteractionResponse::Acknowledge => OracleValue::Number(ALERT_BUTTON1 as f64),
            };
            self.assign_target(&target, res_val, session)?;
        }

        // Continue executing the remaining statements in the block
        self.execute_statements(&ctx.remaining_statements, session)
    }

    /// Assigns a value to a local variable, item bind, or global variable.
    fn assign_target(
        &mut self,
        target: &Expr,
        val: OracleValue,
        session: &mut FormSession,
    ) -> Result<(), RuntimeError> {
        match target {
            Expr::Bind(name) => {
                session.set_bind_value(name, val);
                Ok(())
            }
            Expr::Identifier(name) => {
                let upper = name.to_ascii_uppercase();
                // Check local stack frame first
                if let Some(frame) = self.call_stack.last_mut() {
                    if let std::collections::hash_map::Entry::Occupied(mut e) = frame.variables.entry(upper) {
                        e.insert(val);
                        return Ok(());
                    }
                }
                // Fallback to session bind (e.g. ITEM_NAME)
                session.set_bind_value(name, val);
                Ok(())
            }
            Expr::MemberAccess { object, member } => {
                if let Expr::Identifier(b) = &**object {
                    let bind = format!("{}.{}", b, member);
                    session.set_bind_value(&bind, val);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Evaluates an expression to an OracleValue.
    pub fn eval_expr(
        &mut self,
        expr: &Expr,
        session: &mut FormSession,
    ) -> Result<OracleValue, RuntimeError> {
        match expr {
            Expr::Literal(lit) => Ok(OracleValue::from_literal(lit)),

            Expr::Bind(name) => Ok(session.get_bind_value(name)),

            Expr::Identifier(name) => {
                let upper = name.to_ascii_uppercase();

                // Constants
                match upper.as_str() {
                    "ALERT_BUTTON1" => return Ok(OracleValue::Number(ALERT_BUTTON1 as f64)),
                    "ALERT_BUTTON2" => return Ok(OracleValue::Number(ALERT_BUTTON2 as f64)),
                    "ALERT_BUTTON3" => return Ok(OracleValue::Number(ALERT_BUTTON3 as f64)),
                    "PROPERTY_TRUE" => return Ok(OracleValue::Number(PROPERTY_TRUE as f64)),
                    "PROPERTY_FALSE" => return Ok(OracleValue::Number(PROPERTY_FALSE as f64)),
                    _ => {}
                }

                // Check local stack frame
                if let Some(frame) = self.call_stack.last() {
                    if let Some(val) = frame.variables.get(&upper) {
                        return Ok(val.clone());
                    }
                }

                // Fallback to session bind
                Ok(session.get_bind_value(name))
            }

            Expr::MemberAccess { object, member } => {
                if let Expr::Identifier(b) = &**object {
                    let bind = format!("{}.{}", b, member);
                    Ok(session.get_bind_value(&bind))
                } else {
                    Ok(OracleValue::Null)
                }
            }

            Expr::Parenthesized(inner) => self.eval_expr(inner, session),

            Expr::BinaryOp { left, op, right } => {
                let l_val = self.eval_expr(left, session)?;
                let r_val = self.eval_expr(right, session)?;
                Ok(l_val.eval_binary_op(*op, &r_val))
            }

            Expr::UnaryOp { op, expr } => {
                let val = self.eval_expr(expr, session)?;
                Ok(val.eval_unary_op(*op))
            }

            Expr::FunctionCall { name, args } => {
                let mut eval_args = Vec::new();
                for a in args {
                    eval_args.push(self.eval_expr(a, session)?);
                }

                match BuiltinRegistry::dispatch(name, &eval_args, session, self.interactive) {
                    BuiltinResult::Value(v) => Ok(v),
                    BuiltinResult::Suspend(BuiltinSuspension::ShowAlert { .. }) => {
                        // In non-interactive mode fallback, return ALERT_BUTTON1
                        Ok(OracleValue::Number(ALERT_BUTTON1 as f64))
                    }
                    _ => Ok(OracleValue::Null),
                }
            }
        }
    }
}
