use crate::ast::*;
use crate::lexer::{Lexer, Token, TokenKind};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Unexpected token '{found}' at line {line}, col {col} (expected {expected})")]
    UnexpectedToken {
        expected: String,
        found: String,
        line: usize,
        col: usize,
    },

    #[error("Unexpected end of file")]
    UnexpectedEof,
}

pub struct Parser {
    tokens: Vec<Token>,
    /// XML-unescaped source; token `start`/`end` offsets index into it.
    source: Vec<char>,
    pos: usize,
    pub errors: Vec<String>,
    eof_token: Token,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize();
        let source: Vec<char> = lexer.source().chars().collect();
        let end = source.len();
        Self {
            tokens,
            source,
            pos: 0,
            errors: Vec::new(),
            eof_token: Token {
                kind: TokenKind::Eof,
                text: String::new(),
                line: 0,
                col: 0,
                start: end,
                end,
            },
        }
    }

    /// Original source text covering tokens `from..to` (exclusive), with quotes, spacing
    /// and casing preserved. Used for embedded SQL, which must be sent to the database
    /// exactly as written.
    fn source_between(&self, from: usize, to: usize) -> String {
        if from >= to || from >= self.tokens.len() {
            return String::new();
        }
        let start = self.tokens[from].start;
        let end = self.tokens[(to - 1).min(self.tokens.len() - 1)].end;
        if start >= end || end > self.source.len() {
            return String::new();
        }
        self.source[start..end].iter().collect::<String>().trim().to_string()
    }

    /// Parses `name [CONSTANT] type [:= expr];`, restoring the position on failure.
    fn try_parse_variable_decl(&mut self) -> Option<VariableDecl> {
        let save = self.pos;
        let attempt = (|| -> Result<VariableDecl, ParseError> {
            let name = match self.peek_kind().clone() {
                TokenKind::Identifier(n) => {
                    self.advance();
                    n
                }
                _ => {
                    return Err(ParseError::UnexpectedEof);
                }
            };
            self.match_token(&TokenKind::Constant);
            let data_type = self.parse_data_type()?;
            let default_value = if self.match_token(&TokenKind::Assign) || self.match_token(&TokenKind::Default) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(&TokenKind::Semicolon)?;
            Ok(VariableDecl { name, data_type, default_value })
        })();
        match attempt {
            Ok(v) => Some(v),
            Err(_) => {
                self.pos = save;
                None
            }
        }
    }

    fn is_word(&self, word: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::Identifier(w) if w.eq_ignore_ascii_case(word))
    }

    fn is_word_at(&self, offset: usize, word: &str) -> bool {
        matches!(self.peek_ahead(offset), TokenKind::Identifier(w) if w.eq_ignore_ascii_case(word))
    }

    /// Consumes tokens up to (not including) the next `;` and returns the source text.
    fn take_until_semicolon(&mut self) -> String {
        let from = self.pos;
        while !self.check(&TokenKind::Semicolon) && !self.check(&TokenKind::Eof) {
            self.advance();
        }
        self.source_between(from, self.pos)
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof_token)
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.current().kind
    }

    fn peek_ahead(&self, n: usize) -> &TokenKind {
        self.tokens
            .get(self.pos + n)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.current().clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn check(&self, expected: &TokenKind) -> bool {
        self.peek_kind() == expected
    }

    fn match_token(&mut self, expected: &TokenKind) -> bool {
        if self.check(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<Token, ParseError> {
        if self.check(expected) {
            Ok(self.advance())
        } else {
            let curr = self.current();
            Err(ParseError::UnexpectedToken {
                expected: format!("{:?}", expected),
                found: curr.text.clone(),
                line: curr.line,
                col: curr.col,
            })
        }
    }

    /// Synchronizes on statement boundary token (e.g. Semicolon or End) after a parsing failure.
    fn synchronize_statement(&mut self) -> String {
        let mut raw = String::new();
        while !self.check(&TokenKind::Eof) {
            let tok = self.advance();
            raw.push_str(&tok.text);
            raw.push(' ');
            if tok.kind == TokenKind::Semicolon {
                break;
            }
            if matches!(
                self.peek_kind(),
                TokenKind::End | TokenKind::Exception | TokenKind::Elsif | TokenKind::Else
            ) {
                break;
            }
        }
        raw.trim().to_string()
    }

    /// Parses all PL/SQL units in a source file (e.g. in a .pld or multi-unit script).
    pub fn parse_all_units(&mut self) -> Vec<PlsqlUnit> {
        let mut units = Vec::new();
        while !self.check(&TokenKind::Eof) {
            match self.parse_unit() {
                Ok(unit) => units.push(unit),
                Err(e) => {
                    self.errors.push(e.to_string());
                    let raw = self.synchronize_statement();
                    units.push(PlsqlUnit::StatementList(vec![Statement::Opaque {
                        raw,
                        error: Some(e.to_string()),
                    }]));
                }
            }
        }
        units
    }

    /// Parses a single unit (Trigger body, Procedure, Function, Package Spec, or Package Body).
    pub fn parse_unit(&mut self) -> Result<PlsqlUnit, ParseError> {
        match self.peek_kind() {
            TokenKind::Package => {
                self.advance();
                if self.match_token(&TokenKind::Body) {
                    self.parse_package_body()
                } else {
                    self.parse_package_spec()
                }
            }
            TokenKind::Procedure => self.parse_procedure().map(PlsqlUnit::Procedure),
            TokenKind::Function => self.parse_function().map(PlsqlUnit::Function),
            TokenKind::Declare | TokenKind::Begin => {
                let block = self.parse_block()?;
                // Consume optional trailing semicolon
                self.match_token(&TokenKind::Semicolon);
                Ok(PlsqlUnit::AnonymousBlock(block))
            }
            _ => {
                // Parse as a sequence of statements for triggers lacking an enclosing BEGIN/END wrapper
                let stmts = self.parse_statements_until_end_or_eof();
                Ok(PlsqlUnit::StatementList(stmts))
            }
        }
    }

    fn parse_procedure(&mut self) -> Result<ProcedureDecl, ParseError> {
        self.expect(&TokenKind::Procedure)?;
        let name_tok = self.advance();
        let name = name_tok.text;

        let params = if self.match_token(&TokenKind::OpenParen) {
            let p = self.parse_parameters()?;
            self.expect(&TokenKind::CloseParen)?;
            p
        } else {
            Vec::new()
        };

        // Expect IS or AS
        if !self.match_token(&TokenKind::Is) && !self.match_token(&TokenKind::As) {
            return Err(ParseError::UnexpectedToken {
                expected: "IS or AS".to_string(),
                found: self.current().text.clone(),
                line: self.current().line,
                col: self.current().col,
            });
        }

        let declarations = self.parse_declarations()?;
        self.expect(&TokenKind::Begin)?;
        let body = self.parse_statements_until_exception_or_end()?;
        let exception_handlers = if self.match_token(&TokenKind::Exception) {
            self.parse_exception_handlers()?
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::End)?;
        // Optional name after END
        if let TokenKind::Identifier(_) = self.peek_kind() {
            self.advance();
        }
        self.expect(&TokenKind::Semicolon)?;

        Ok(ProcedureDecl {
            name,
            params,
            declarations,
            body,
            exception_handlers,
        })
    }

    fn parse_function(&mut self) -> Result<FunctionDecl, ParseError> {
        self.expect(&TokenKind::Function)?;
        let name = self.advance().text;

        let params = if self.match_token(&TokenKind::OpenParen) {
            let p = self.parse_parameters()?;
            self.expect(&TokenKind::CloseParen)?;
            p
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::Return)?;
        let return_type = self.parse_data_type()?;

        if !self.match_token(&TokenKind::Is) && !self.match_token(&TokenKind::As) {
            return Err(ParseError::UnexpectedToken {
                expected: "IS or AS".to_string(),
                found: self.current().text.clone(),
                line: self.current().line,
                col: self.current().col,
            });
        }

        let declarations = self.parse_declarations()?;
        self.expect(&TokenKind::Begin)?;
        let body = self.parse_statements_until_exception_or_end()?;
        let exception_handlers = if self.match_token(&TokenKind::Exception) {
            self.parse_exception_handlers()?
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::End)?;
        if let TokenKind::Identifier(_) = self.peek_kind() {
            self.advance();
        }
        self.expect(&TokenKind::Semicolon)?;

        Ok(FunctionDecl {
            name,
            params,
            return_type,
            declarations,
            body,
            exception_handlers,
        })
    }

    fn parse_package_spec(&mut self) -> Result<PlsqlUnit, ParseError> {
        let name = self.advance().text;
        if !self.match_token(&TokenKind::Is) && !self.match_token(&TokenKind::As) {
            return Err(ParseError::UnexpectedToken {
                expected: "IS or AS".to_string(),
                found: self.current().text.clone(),
                line: self.current().line,
                col: self.current().col,
            });
        }

        let mut items = Vec::new();
        while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Procedure) {
                // Spec procedure signature
                self.advance();
                let p_name = self.advance().text;
                let params = if self.match_token(&TokenKind::OpenParen) {
                    let p = self.parse_parameters()?;
                    self.expect(&TokenKind::CloseParen)?;
                    p
                } else {
                    Vec::new()
                };
                self.expect(&TokenKind::Semicolon)?;
                items.push(PackageSpecItem::Procedure(ProcedureDecl {
                    name: p_name,
                    params,
                    declarations: vec![],
                    body: vec![],
                    exception_handlers: vec![],
                }));
            } else if self.check(&TokenKind::Function) {
                self.advance();
                let f_name = self.advance().text;
                let params = if self.match_token(&TokenKind::OpenParen) {
                    let p = self.parse_parameters()?;
                    self.expect(&TokenKind::CloseParen)?;
                    p
                } else {
                    Vec::new()
                };
                self.expect(&TokenKind::Return)?;
                let return_type = self.parse_data_type()?;
                self.expect(&TokenKind::Semicolon)?;
                items.push(PackageSpecItem::Function(FunctionDecl {
                    name: f_name,
                    params,
                    return_type,
                    declarations: vec![],
                    body: vec![],
                    exception_handlers: vec![],
                }));
            } else if let Some(v) = self.try_parse_variable_decl() {
                items.push(PackageSpecItem::Variable(v));
            } else {
                // Types, pragmas, cursors in specs: skip (recorded as a parse note).
                let raw = self.synchronize_statement();
                self.errors.push(format!("Skipped package spec item: {}", raw));
            }
        }

        self.expect(&TokenKind::End)?;
        if let TokenKind::Identifier(_) = self.peek_kind() {
            self.advance();
        }
        self.expect(&TokenKind::Semicolon)?;

        Ok(PlsqlUnit::PackageSpec(PackageSpecDecl { name, items }))
    }

    fn parse_package_body(&mut self) -> Result<PlsqlUnit, ParseError> {
        let name = self.advance().text;
        if !self.match_token(&TokenKind::Is) && !self.match_token(&TokenKind::As) {
            return Err(ParseError::UnexpectedToken {
                expected: "IS or AS".to_string(),
                found: self.current().text.clone(),
                line: self.current().line,
                col: self.current().col,
            });
        }

        let mut items = Vec::new();
        while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Procedure) {
                let proc = self.parse_procedure()?;
                items.push(PackageBodyItem::Procedure(proc));
            } else if self.check(&TokenKind::Function) {
                let func = self.parse_function()?;
                items.push(PackageBodyItem::Function(func));
            } else if self.check(&TokenKind::Begin) {
                // Package initialization section: BEGIN ... END pkg;
                let raw = self.synchronize_statement();
                self.errors.push(format!("Package initialization section is not supported: {}", raw));
            } else if let Some(v) = self.try_parse_variable_decl() {
                items.push(PackageBodyItem::Variable(v));
            } else {
                let raw = self.synchronize_statement();
                self.errors.push(format!("Skipped package body item: {}", raw));
            }
        }

        self.expect(&TokenKind::End)?;
        if let TokenKind::Identifier(_) = self.peek_kind() {
            self.advance();
        }
        self.expect(&TokenKind::Semicolon)?;

        Ok(PlsqlUnit::PackageBody(PackageBodyDecl { name, items }))
    }

    fn parse_parameters(&mut self) -> Result<Vec<ParameterDecl>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&TokenKind::CloseParen) && !self.check(&TokenKind::Eof) {
            let p_name = self.advance().text;
            let mode = if self.match_token(&TokenKind::In) {
                if self.is_word("OUT") {
                    self.advance();
                    ParamMode::InOut
                } else {
                    ParamMode::In
                }
            } else if self.is_word("OUT") {
                self.advance();
                ParamMode::Out
            } else {
                ParamMode::In
            };
            if self.is_word("NOCOPY") {
                self.advance();
            }

            let data_type = self.parse_data_type()?;
            let default_value = if self.match_token(&TokenKind::Assign) || self.match_token(&TokenKind::Default) {
                Some(self.parse_expr()?)
            } else {
                None
            };

            params.push(ParameterDecl {
                name: p_name,
                mode,
                data_type,
                default_value,
            });

            if !self.match_token(&TokenKind::Comma) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_data_type(&mut self) -> Result<DataType, ParseError> {
        let mut type_str = self.advance().text;

        // Check for size specification like VARCHAR2(100) or NUMBER(10, 2)
        if self.match_token(&TokenKind::OpenParen) {
            type_str.push('(');
            while !self.check(&TokenKind::CloseParen) && !self.check(&TokenKind::Eof) {
                let t = self.advance();
                type_str.push_str(&t.text);
            }
            self.expect(&TokenKind::CloseParen)?;
            type_str.push(')');
        }

        // Check for %TYPE or %ROWTYPE
        if self.match_token(&TokenKind::Percent) {
            let attr = self.advance().text.to_ascii_uppercase();
            if attr == "TYPE" {
                return Ok(DataType::PercentType(type_str));
            } else if attr == "ROWTYPE" {
                return Ok(DataType::PercentRowType(type_str));
            }
        }

        Ok(DataType::Primitive(type_str))
    }

    fn parse_declarations(&mut self) -> Result<Vec<Declaration>, ParseError> {
        let mut decls = Vec::new();
        while !self.check(&TokenKind::Begin) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Cursor) {
                self.advance();
                let c_name = self.advance().text;
                let params = if self.match_token(&TokenKind::OpenParen) {
                    let p = self.parse_parameters()?;
                    self.expect(&TokenKind::CloseParen)?;
                    p
                } else {
                    Vec::new()
                };
                let query = if self.match_token(&TokenKind::Is) {
                    self.take_until_semicolon()
                } else {
                    String::new()
                };
                self.expect(&TokenKind::Semicolon)?;
                decls.push(Declaration::Cursor(CursorDecl {
                    name: c_name,
                    params,
                    query,
                }));
            } else if let TokenKind::Identifier(name) = self.peek_kind().clone() {
                self.advance();
                let is_const = self.match_token(&TokenKind::Constant);
                let data_type = self.parse_data_type()?;
                let default_val = if self.match_token(&TokenKind::Assign) || self.match_token(&TokenKind::Default) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(&TokenKind::Semicolon)?;
                let var_decl = VariableDecl {
                    name,
                    data_type,
                    default_value: default_val,
                };
                if is_const {
                    decls.push(Declaration::Constant(var_decl));
                } else {
                    decls.push(Declaration::Variable(var_decl));
                }
            } else {
                let raw = self.synchronize_statement();
                decls.push(Declaration::Variable(VariableDecl {
                    name: raw,
                    data_type: DataType::Primitive("UNKNOWN".to_string()),
                    default_value: None,
                }));
            }
        }
        Ok(decls)
    }

    fn parse_block(&mut self) -> Result<BlockStmt, ParseError> {
        let declarations = if self.match_token(&TokenKind::Declare) {
            self.parse_declarations()?
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::Begin)?;
        let body = self.parse_statements_until_exception_or_end()?;

        let exception_handlers = if self.match_token(&TokenKind::Exception) {
            self.parse_exception_handlers()?
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::End)?;
        if let TokenKind::Identifier(_) = self.peek_kind() {
            self.advance();
        }

        Ok(BlockStmt {
            declarations,
            body,
            exception_handlers,
        })
    }

    fn parse_exception_handlers(&mut self) -> Result<Vec<ExceptionHandler>, ParseError> {
        let mut handlers = Vec::new();
        while self.match_token(&TokenKind::When) {
            let mut exceptions = Vec::new();
            while !self.check(&TokenKind::Then) && !self.check(&TokenKind::Eof) {
                if self.check(&TokenKind::Others) {
                    self.advance();
                    exceptions.push("OTHERS".to_string());
                } else if let TokenKind::Identifier(e) = self.peek_kind().clone() {
                    self.advance();
                    exceptions.push(e);
                } else {
                    self.advance();
                }
                if self.match_token(&TokenKind::Or) {
                    continue;
                }
            }
            self.expect(&TokenKind::Then)?;
            let mut body = Vec::new();
            while !self.check(&TokenKind::When) && !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
                body.push(self.parse_statement()?);
            }
            handlers.push(ExceptionHandler { exceptions, body });
        }
        Ok(handlers)
    }

    fn parse_statements_until_exception_or_end(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::Exception) && !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
            match self.parse_statement() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) => {
                    self.errors.push(e.to_string());
                    let raw = self.synchronize_statement();
                    stmts.push(Statement::Opaque {
                        raw,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
        Ok(stmts)
    }

    fn parse_statements_until_end_or_eof(&mut self) -> Vec<Statement> {
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
            match self.parse_statement() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) => {
                    self.errors.push(e.to_string());
                    let raw = self.synchronize_statement();
                    stmts.push(Statement::Opaque {
                        raw,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
        stmts
    }

    pub fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.peek_kind() {
            TokenKind::Null => {
                self.advance();
                self.expect(&TokenKind::Semicolon)?;
                Ok(Statement::Null)
            }
            TokenKind::Return => {
                self.advance();
                let expr = if !self.check(&TokenKind::Semicolon) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(&TokenKind::Semicolon)?;
                Ok(Statement::Return(expr))
            }
            TokenKind::Raise => {
                self.advance();
                let exc = if !self.check(&TokenKind::Semicolon) {
                    Some(self.advance().text)
                } else {
                    None
                };
                self.expect(&TokenKind::Semicolon)?;
                Ok(Statement::Raise(exc))
            }
            TokenKind::If => self.parse_if_statement(),
            TokenKind::Identifier(w) if w.eq_ignore_ascii_case("EXIT") => {
                self.advance();
                let condition = if self.match_token(&TokenKind::When) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(&TokenKind::Semicolon)?;
                Ok(Statement::Exit { condition })
            }
            TokenKind::Identifier(w) if w.eq_ignore_ascii_case("CASE") => self.parse_case_statement(),
            TokenKind::Identifier(w) if w.eq_ignore_ascii_case("OPEN") => {
                self.advance();
                let cursor = self.advance().text;
                let args = if self.match_token(&TokenKind::OpenParen) {
                    let a = self.parse_expr_list()?;
                    self.expect(&TokenKind::CloseParen)?;
                    a
                } else {
                    Vec::new()
                };
                if !self.check(&TokenKind::Semicolon) {
                    // OPEN c FOR <query> (ref cursors) is not supported.
                    let raw = self.synchronize_statement();
                    return Ok(Statement::Opaque { raw, error: Some("OPEN ... FOR is not supported".to_string()) });
                }
                self.expect(&TokenKind::Semicolon)?;
                Ok(Statement::OpenCursor { cursor, args })
            }
            TokenKind::Identifier(w) if w.eq_ignore_ascii_case("FETCH") => {
                self.advance();
                let cursor = self.advance().text;
                if self.is_word("BULK") {
                    let raw = self.synchronize_statement();
                    return Ok(Statement::Opaque { raw, error: Some("FETCH BULK COLLECT is not supported".to_string()) });
                }
                self.expect(&TokenKind::Into)?;
                let into = self.parse_expr_list()?;
                self.expect(&TokenKind::Semicolon)?;
                Ok(Statement::FetchCursor { cursor, into })
            }
            TokenKind::Identifier(w) if w.eq_ignore_ascii_case("CLOSE") => {
                self.advance();
                let cursor = self.advance().text;
                self.expect(&TokenKind::Semicolon)?;
                Ok(Statement::CloseCursor { cursor })
            }
            TokenKind::Identifier(w)
                if ["INSERT", "UPDATE", "DELETE", "MERGE", "LOCK", "COMMIT", "ROLLBACK", "SAVEPOINT"]
                    .iter()
                    .any(|k| w.eq_ignore_ascii_case(k))
                    // `commit_form;` etc. are calls, and `update_x := 1` is an assignment.
                    && !matches!(self.peek_ahead(1), TokenKind::Assign | TokenKind::OpenParen | TokenKind::Dot) =>
            {
                let kind = match w.to_ascii_uppercase().as_str() {
                    "INSERT" => DmlKind::Insert,
                    "UPDATE" => DmlKind::Update,
                    "DELETE" => DmlKind::Delete,
                    "COMMIT" => DmlKind::Commit,
                    "ROLLBACK" => DmlKind::Rollback,
                    _ => DmlKind::Other,
                };
                let raw_sql = self.take_until_semicolon();
                self.expect(&TokenKind::Semicolon)?;
                Ok(Statement::Sql(SqlStatement::Dml { kind, raw_sql }))
            }
            TokenKind::Loop => self.parse_loop_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::Select => self.parse_select_into(),
            TokenKind::Declare | TokenKind::Begin => {
                let block = self.parse_block()?;
                self.match_token(&TokenKind::Semicolon);
                Ok(Statement::Block(block))
            }
            TokenKind::BindVariable(_) => {
                // :BLOCK.ITEM := expr;
                let target_expr = self.parse_primary_expr()?;
                if self.match_token(&TokenKind::Assign) {
                    let val = self.parse_expr()?;
                    self.expect(&TokenKind::Semicolon)?;
                    Ok(Statement::Assignment {
                        target: target_expr,
                        value: val,
                    })
                } else {
                    let raw = self.synchronize_statement();
                    Ok(Statement::Opaque {
                        raw,
                        error: Some("Expected assignment operator :=".to_string()),
                    })
                }
            }
            TokenKind::Identifier(_) => {
                // Could be assignment (var := expr;) or procedure call (proc(a, b);)
                if self.peek_ahead(1) == &TokenKind::Assign {
                    let target_expr = self.parse_primary_expr()?;
                    self.expect(&TokenKind::Assign)?;
                    let value = self.parse_expr()?;
                    self.expect(&TokenKind::Semicolon)?;
                    Ok(Statement::Assignment {
                        target: target_expr,
                        value,
                    })
                } else {
                    let mut target = self.advance().text;
                    while self.match_token(&TokenKind::Dot) {
                        target.push('.');
                        target.push_str(&self.advance().text);
                    }
                    let args = if self.match_token(&TokenKind::OpenParen) {
                        let a = self.parse_expr_list()?;
                        self.expect(&TokenKind::CloseParen)?;
                        a
                    } else {
                        Vec::new()
                    };
                    self.expect(&TokenKind::Semicolon)?;
                    Ok(Statement::Call { target, args })
                }
            }
            _ => {
                let raw = self.synchronize_statement();
                Ok(Statement::Opaque {
                    raw,
                    error: Some("Unrecognized statement".to_string()),
                })
            }
        }
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&TokenKind::If)?;
        let condition = self.parse_expr()?;
        self.expect(&TokenKind::Then)?;

        let mut then_branch = Vec::new();
        while !matches!(
            self.peek_kind(),
            TokenKind::Elsif | TokenKind::Else | TokenKind::End | TokenKind::Eof
        ) {
            then_branch.push(self.parse_statement()?);
        }

        let mut elsifs = Vec::new();
        while self.match_token(&TokenKind::Elsif) {
            let cond = self.parse_expr()?;
            self.expect(&TokenKind::Then)?;
            let mut branch = Vec::new();
            while !matches!(
                self.peek_kind(),
                TokenKind::Elsif | TokenKind::Else | TokenKind::End | TokenKind::Eof
            ) {
                branch.push(self.parse_statement()?);
            }
            elsifs.push((cond, branch));
        }

        let else_branch = if self.match_token(&TokenKind::Else) {
            let mut branch = Vec::new();
            while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
                branch.push(self.parse_statement()?);
            }
            Some(branch)
        } else {
            None
        };

        self.expect(&TokenKind::End)?;
        self.expect(&TokenKind::If)?;
        self.expect(&TokenKind::Semicolon)?;

        Ok(Statement::If {
            condition,
            then_branch,
            elsifs,
            else_branch,
        })
    }

    fn parse_loop_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&TokenKind::Loop)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
            body.push(self.parse_statement()?);
        }
        self.expect(&TokenKind::End)?;
        self.expect(&TokenKind::Loop)?;
        self.expect(&TokenKind::Semicolon)?;
        Ok(Statement::Loop { body })
    }

    fn parse_while_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&TokenKind::While)?;
        let condition = self.parse_expr()?;
        self.expect(&TokenKind::Loop)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
            body.push(self.parse_statement()?);
        }
        self.expect(&TokenKind::End)?;
        self.expect(&TokenKind::Loop)?;
        self.expect(&TokenKind::Semicolon)?;
        Ok(Statement::WhileLoop { condition, body })
    }

    fn parse_loop_body(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.expect(&TokenKind::Loop)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
            body.push(self.parse_statement()?);
        }
        self.expect(&TokenKind::End)?;
        self.expect(&TokenKind::Loop)?;
        self.expect(&TokenKind::Semicolon)?;
        Ok(body)
    }

    fn parse_for_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&TokenKind::For)?;
        let var_name = self.advance().text;
        self.expect(&TokenKind::In)?;

        // FOR rec IN (SELECT ...) LOOP
        if self.check(&TokenKind::OpenParen) && self.peek_ahead(1) == &TokenKind::Select {
            self.advance();
            let from = self.pos;
            let mut depth = 1usize;
            while !self.check(&TokenKind::Eof) {
                match self.peek_kind() {
                    TokenKind::OpenParen => depth += 1,
                    TokenKind::CloseParen => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                self.advance();
            }
            let query = self.source_between(from, self.pos);
            self.expect(&TokenKind::CloseParen)?;
            let body = self.parse_loop_body()?;
            return Ok(Statement::CursorForLoop { var_name, cursor: None, args: vec![], query: Some(query), body });
        }

        // FOR rec IN cursor_name[(args)] LOOP  (no `..` range follows)
        if let TokenKind::Identifier(cursor) = self.peek_kind().clone() {
            let mut look = 1;
            if self.peek_ahead(1) == &TokenKind::OpenParen {
                let mut depth = 0usize;
                loop {
                    match self.peek_ahead(look) {
                        TokenKind::OpenParen => depth += 1,
                        TokenKind::CloseParen => {
                            depth -= 1;
                            if depth == 0 {
                                look += 1;
                                break;
                            }
                        }
                        TokenKind::Eof => break,
                        _ => {}
                    }
                    look += 1;
                }
            }
            if self.peek_ahead(look) == &TokenKind::Loop {
                self.advance();
                let args = if self.match_token(&TokenKind::OpenParen) {
                    let a = self.parse_expr_list()?;
                    self.expect(&TokenKind::CloseParen)?;
                    a
                } else {
                    Vec::new()
                };
                let body = self.parse_loop_body()?;
                return Ok(Statement::CursorForLoop { var_name, cursor: Some(cursor), args, query: None, body });
            }
        }

        let reverse = self.match_token(&TokenKind::Reverse);

        let lower = self.parse_primary_expr()?;
        self.expect(&TokenKind::DotDot)?;
        let upper = self.parse_primary_expr()?;

        self.expect(&TokenKind::Loop)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
            body.push(self.parse_statement()?);
        }
        self.expect(&TokenKind::End)?;
        self.expect(&TokenKind::Loop)?;
        self.expect(&TokenKind::Semicolon)?;

        Ok(Statement::ForLoop {
            var_name,
            reverse,
            lower,
            upper,
            body,
        })
    }

    fn parse_select_into(&mut self) -> Result<Statement, ParseError> {
        let select_tok = self.pos;
        self.expect(&TokenKind::Select)?;
        let select_from = self.pos;
        let mut depth = 0usize;
        while !(depth == 0 && self.check(&TokenKind::Into)) && !self.check(&TokenKind::Eof) && !self.check(&TokenKind::Semicolon) {
            match self.peek_kind() {
                TokenKind::OpenParen => depth += 1,
                TokenKind::CloseParen => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.advance();
        }
        let select_clause = self.source_between(select_from, self.pos);

        if !self.check(&TokenKind::Into) {
            // A bare SELECT without INTO is not valid PL/SQL.
            let raw = format!("{} {}", self.source_between(select_tok, self.pos), self.take_until_semicolon());
            self.match_token(&TokenKind::Semicolon);
            return Ok(Statement::Opaque { raw, error: Some("SELECT without INTO".to_string()) });
        }
        self.expect(&TokenKind::Into)?;
        if self.is_word("BULK") {
            let raw = self.synchronize_statement();
            return Ok(Statement::Opaque { raw, error: Some("BULK COLLECT is not supported".to_string()) });
        }
        let into_targets = self.parse_expr_list()?;

        self.expect(&TokenKind::From)?;
        let from_start = self.pos;
        let mut depth = 0usize;
        while !(depth == 0 && self.check(&TokenKind::Where)) && !self.check(&TokenKind::Semicolon) && !self.check(&TokenKind::Eof) {
            match self.peek_kind() {
                TokenKind::OpenParen => depth += 1,
                TokenKind::CloseParen => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.advance();
        }
        let from_clause = self.source_between(from_start, self.pos);

        let where_clause = if self.match_token(&TokenKind::Where) {
            Some(self.take_until_semicolon())
        } else {
            None
        };

        self.expect(&TokenKind::Semicolon)?;

        // The executable query: the original text with the INTO clause removed.
        let raw_sql = format!(
            "SELECT {} FROM {}{}",
            select_clause,
            from_clause,
            where_clause.as_ref().map(|w| format!(" WHERE {}", w)).unwrap_or_default()
        );

        Ok(Statement::Sql(SqlStatement::SelectInto {
            select_clause,
            into_targets,
            from_clause,
            where_clause,
            raw_sql,
        }))
    }

    fn parse_case_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // CASE
        let selector = if self.check(&TokenKind::When) { None } else { Some(self.parse_expr()?) };
        let mut whens = Vec::new();
        while self.match_token(&TokenKind::When) {
            let cond = self.parse_expr()?;
            self.expect(&TokenKind::Then)?;
            let mut body = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::When | TokenKind::Else | TokenKind::End | TokenKind::Eof) {
                body.push(self.parse_statement()?);
            }
            whens.push((cond, body));
        }
        let else_branch = if self.match_token(&TokenKind::Else) {
            let mut body = Vec::new();
            while !self.check(&TokenKind::End) && !self.check(&TokenKind::Eof) {
                body.push(self.parse_statement()?);
            }
            Some(body)
        } else {
            None
        };
        self.expect(&TokenKind::End)?;
        if self.is_word("CASE") {
            self.advance();
        }
        self.expect(&TokenKind::Semicolon)?;
        Ok(Statement::Case { selector, whens, else_branch })
    }

    fn parse_expr_list(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut list = Vec::new();
        while !self.check(&TokenKind::CloseParen)
            && !self.check(&TokenKind::From)
            && !self.check(&TokenKind::Semicolon)
            && !self.check(&TokenKind::Eof)
        {
            if let TokenKind::Identifier(name) = self.peek_kind().clone() {
                if self.peek_ahead(1) == &TokenKind::Arrow {
                    self.advance();
                    self.advance();
                    let value = self.parse_expr()?;
                    list.push(Expr::NamedArg { name, value: Box::new(value) });
                    if !self.match_token(&TokenKind::Comma) {
                        break;
                    }
                    continue;
                }
            }
            list.push(self.parse_expr()?);
            if !self.match_token(&TokenKind::Comma) {
                break;
            }
        }
        Ok(list)
    }

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;
        while self.match_token(&TokenKind::Or) {
            let right = self.parse_and_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison_expr()?;
        while self.match_token(&TokenKind::And) {
            let right = self.parse_comparison_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_concat_expr()?;

        // [NOT] IN / [NOT] BETWEEN / NOT LIKE
        let negated = if self.check(&TokenKind::Not)
            && (matches!(self.peek_ahead(1), TokenKind::In | TokenKind::Like) || self.is_word_at(1, "BETWEEN"))
        {
            self.advance();
            true
        } else {
            false
        };
        if self.match_token(&TokenKind::In) {
            self.expect(&TokenKind::OpenParen)?;
            let list = self.parse_expr_list()?;
            self.expect(&TokenKind::CloseParen)?;
            return Ok(Expr::InList { expr: Box::new(left), list, negated });
        }
        if self.is_word("BETWEEN") {
            self.advance();
            let low = self.parse_concat_expr()?;
            self.expect(&TokenKind::And)?;
            let high = self.parse_concat_expr()?;
            return Ok(Expr::Between { expr: Box::new(left), low: Box::new(low), high: Box::new(high), negated });
        }
        if negated {
            self.expect(&TokenKind::Like)?;
            let right = self.parse_concat_expr()?;
            let like = Expr::BinaryOp { left: Box::new(left), op: BinaryOp::Like, right: Box::new(right) };
            return Ok(Expr::UnaryOp { op: UnaryOp::Not, expr: Box::new(like) });
        }

        let op = if self.match_token(&TokenKind::Equal) {
            Some(BinaryOp::Equal)
        } else if self.match_token(&TokenKind::NotEqual) {
            Some(BinaryOp::NotEqual)
        } else if self.match_token(&TokenKind::LessThan) {
            Some(BinaryOp::LessThan)
        } else if self.match_token(&TokenKind::LessThanOrEqual) {
            Some(BinaryOp::LessThanOrEqual)
        } else if self.match_token(&TokenKind::GreaterThan) {
            Some(BinaryOp::GreaterThan)
        } else if self.match_token(&TokenKind::GreaterThanOrEqual) {
            Some(BinaryOp::GreaterThanOrEqual)
        } else if self.match_token(&TokenKind::Like) {
            Some(BinaryOp::Like)
        } else if self.match_token(&TokenKind::Is) {
            // IS NULL / IS NOT NULL are two-valued tests, not comparisons with NULL
            // (`x = NULL` is never true in Oracle).
            let op = if self.match_token(&TokenKind::Not) { UnaryOp::IsNotNull } else { UnaryOp::IsNull };
            self.expect(&TokenKind::Null)?;
            return Ok(Expr::UnaryOp { op, expr: Box::new(left) });
        } else {
            None
        };

        if let Some(binary_op) = op {
            let right = self.parse_concat_expr()?;
            Ok(Expr::BinaryOp {
                left: Box::new(left),
                op: binary_op,
                right: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_concat_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive_expr()?;
        while self.match_token(&TokenKind::Concat) {
            let right = self.parse_additive_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOp::Concat,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative_expr()?;
        while let Some(op) = match self.peek_kind() {
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Subtract),
            _ => None,
        } {
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary_expr()?;
        while let Some(op) = match self.peek_kind() {
            TokenKind::Star => Some(BinaryOp::Multiply),
            TokenKind::Slash => Some(BinaryOp::Divide),
            _ => None,
        } {
            self.advance();
            let right = self.parse_unary_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        if self.match_token(&TokenKind::Not) {
            let expr = self.parse_unary_expr()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            })
        } else if self.match_token(&TokenKind::Minus) {
            let expr = self.parse_unary_expr()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Negate,
                expr: Box::new(expr),
            })
        } else {
            self.parse_primary_expr()
        }
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::Null))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::Boolean(true)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::Boolean(false)))
            }
            TokenKind::StringLiteral(s) => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::String(s)))
            }
            TokenKind::NumberLiteral(n) => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::Number(n)))
            }
            TokenKind::BindVariable(b) => {
                self.advance();
                Ok(Expr::Bind(b))
            }
            TokenKind::OpenParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::CloseParen)?;
                Ok(Expr::Parenthesized(Box::new(expr)))
            }
            TokenKind::Identifier(id) if id.eq_ignore_ascii_case("CASE") => {
                self.advance();
                let selector = if self.check(&TokenKind::When) { None } else { Some(Box::new(self.parse_expr()?)) };
                let mut whens = Vec::new();
                while self.match_token(&TokenKind::When) {
                    let cond = self.parse_expr()?;
                    self.expect(&TokenKind::Then)?;
                    let val = self.parse_expr()?;
                    whens.push((cond, val));
                }
                let else_expr = if self.match_token(&TokenKind::Else) { Some(Box::new(self.parse_expr()?)) } else { None };
                self.expect(&TokenKind::End)?;
                Ok(Expr::Case { selector, whens, else_expr })
            }
            TokenKind::Identifier(id) if self.peek_ahead(1) == &TokenKind::Percent => {
                // SQL%ROWCOUNT, c1%NOTFOUND, ...
                self.advance();
                self.advance();
                let attr = self.advance().text.to_ascii_uppercase();
                Ok(Expr::Attribute { object: id.to_ascii_uppercase(), attr })
            }
            TokenKind::Identifier(id) => {
                self.advance();
                let mut expr = Expr::Identifier(id.clone());

                // Check for function call or member access
                loop {
                    if self.match_token(&TokenKind::Dot) {
                        let member = self.advance().text;
                        expr = Expr::MemberAccess {
                            object: Box::new(expr),
                            member,
                        };
                    } else if self.match_token(&TokenKind::OpenParen) {
                        let args = self.parse_expr_list()?;
                        self.expect(&TokenKind::CloseParen)?;
                        let name = dotted_name(&expr).unwrap_or_else(|| id.clone());
                        expr = Expr::FunctionCall { name, args };
                    } else {
                        break;
                    }
                }
                Ok(expr)
            }
            _ => {
                let tok = self.advance();
                Ok(Expr::Identifier(tok.text))
            }
        }
    }
}

/// Flattens `a.b.c` member-access chains into the dotted name `a.b.c`.
fn dotted_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(s) => Some(s.clone()),
        Expr::MemberAccess { object, member } => dotted_name(object).map(|o| format!("{}.{}", o, member)),
        _ => None,
    }
}
