use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlsqlUnit {
    AnonymousBlock(BlockStmt),
    Procedure(ProcedureDecl),
    Function(FunctionDecl),
    PackageSpec(PackageSpecDecl),
    PackageBody(PackageBodyDecl),
    StatementList(Vec<Statement>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureDecl {
    pub name: String,
    pub params: Vec<ParameterDecl>,
    pub declarations: Vec<Declaration>,
    pub body: Vec<Statement>,
    pub exception_handlers: Vec<ExceptionHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<ParameterDecl>,
    pub return_type: DataType,
    pub declarations: Vec<Declaration>,
    pub body: Vec<Statement>,
    pub exception_handlers: Vec<ExceptionHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageSpecDecl {
    pub name: String,
    pub items: Vec<PackageSpecItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PackageSpecItem {
    Procedure(ProcedureDecl),
    Function(FunctionDecl),
    Variable(VariableDecl),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageBodyDecl {
    pub name: String,
    pub items: Vec<PackageBodyItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PackageBodyItem {
    Procedure(ProcedureDecl),
    Function(FunctionDecl),
    Variable(VariableDecl),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockStmt {
    pub declarations: Vec<Declaration>,
    pub body: Vec<Statement>,
    pub exception_handlers: Vec<ExceptionHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Declaration {
    Variable(VariableDecl),
    Constant(VariableDecl),
    Cursor(CursorDecl),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDecl {
    pub name: String,
    pub data_type: DataType,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorDecl {
    pub name: String,
    pub params: Vec<ParameterDecl>,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterDecl {
    pub name: String,
    pub mode: ParamMode,
    pub data_type: DataType,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ParamMode {
    #[default]
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataType {
    Primitive(String),
    PercentType(String),     // e.g. emp.salary%TYPE
    PercentRowType(String),  // e.g. emp%ROWTYPE
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExceptionHandler {
    pub exceptions: Vec<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Assignment {
        target: Expr,
        value: Expr,
    },
    Call {
        target: String,
        args: Vec<Expr>,
    },
    If {
        condition: Expr,
        then_branch: Vec<Statement>,
        elsifs: Vec<(Expr, Vec<Statement>)>,
        else_branch: Option<Vec<Statement>>,
    },
    Loop {
        body: Vec<Statement>,
    },
    WhileLoop {
        condition: Expr,
        body: Vec<Statement>,
    },
    ForLoop {
        var_name: String,
        reverse: bool,
        lower: Expr,
        upper: Expr,
        body: Vec<Statement>,
    },
    Return(Option<Expr>),
    Raise(Option<String>),
    Null,
    Block(BlockStmt),
    Sql(SqlStatement),
    /// `EXIT;` or `EXIT WHEN cond;` (leaves the innermost loop).
    Exit {
        condition: Option<Expr>,
    },
    /// `CASE [selector] WHEN .. THEN .. [ELSE ..] END CASE;`
    Case {
        selector: Option<Expr>,
        whens: Vec<(Expr, Vec<Statement>)>,
        else_branch: Option<Vec<Statement>>,
    },
    /// `OPEN cursor [(args)];`
    OpenCursor {
        cursor: String,
        args: Vec<Expr>,
    },
    /// `FETCH cursor INTO targets;`
    FetchCursor {
        cursor: String,
        into: Vec<Expr>,
    },
    /// `CLOSE cursor;`
    CloseCursor {
        cursor: String,
    },
    /// `FOR rec IN cursor[(args)] LOOP` or `FOR rec IN (SELECT ...) LOOP`
    CursorForLoop {
        var_name: String,
        cursor: Option<String>,
        args: Vec<Expr>,
        query: Option<String>,
        body: Vec<Statement>,
    },
    Opaque {
        raw: String,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlStatement {
    SelectInto {
        select_clause: String,
        into_targets: Vec<Expr>,
        from_clause: String,
        where_clause: Option<String>,
        raw_sql: String,
    },
    Dml {
        kind: DmlKind,
        raw_sql: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DmlKind {
    Insert,
    Update,
    Delete,
    Commit,
    Rollback,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal(LiteralValue),
    Identifier(String),
    Bind(String),
    MemberAccess {
        object: Box<Expr>,
        member: String,
    },
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
    Parenthesized(Box<Expr>),
    /// `expr [NOT] IN (a, b, ...)`
    InList {
        expr: Box<Expr>,
        list: Vec<Expr>,
        negated: bool,
    },
    /// `expr [NOT] BETWEEN low AND high`
    Between {
        expr: Box<Expr>,
        low: Box<Expr>,
        high: Box<Expr>,
        negated: bool,
    },
    /// Cursor / implicit-cursor attribute: `SQL%ROWCOUNT`, `c1%NOTFOUND`, ...
    Attribute {
        object: String,
        attr: String,
    },
    /// Named argument in a call: `p_name => value`.
    NamedArg {
        name: String,
        value: Box<Expr>,
    },
    /// `CASE [selector] WHEN a THEN x ... [ELSE y] END`
    Case {
        selector: Option<Box<Expr>>,
        whens: Vec<(Expr, Expr)>,
        else_expr: Option<Box<Expr>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LiteralValue {
    String(String),
    Number(String),
    Boolean(bool),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Concat,
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    And,
    Or,
    Like,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Negate,
    /// `expr IS NULL` (always TRUE or FALSE, never NULL)
    IsNull,
    /// `expr IS NOT NULL`
    IsNotNull,
}
