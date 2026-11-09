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
}
