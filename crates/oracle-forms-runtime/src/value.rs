use oracle_forms_db::model::SqlValue;
use oracle_forms_plsql::ast::{BinaryOp, LiteralValue, UnaryOp};
use serde::{Deserialize, Serialize};

/// Oracle runtime value supporting 3-valued logic and Oracle NULL semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum OracleValue {
    #[default]
    Null,
    Number(f64),
    Varchar2(String),
    Date(String),
    Boolean(Option<bool>), // Some(true), Some(false), None (NULL)
}

impl OracleValue {
    /// Creates a Varchar2 value, enforcing the Oracle invariant that empty string `""` is strictly `NULL`.
    pub fn varchar2(s: impl Into<String>) -> Self {
        let text = s.into();
        if text.is_empty() {
            OracleValue::Null
        } else {
            OracleValue::Varchar2(text)
        }
    }

    pub fn number(n: f64) -> Self {
        OracleValue::Number(n)
    }

    pub fn boolean(b: bool) -> Self {
        OracleValue::Boolean(Some(b))
    }

    pub fn null_boolean() -> Self {
        OracleValue::Boolean(None)
    }

    pub fn is_null(&self) -> bool {
        match self {
            OracleValue::Null => true,
            OracleValue::Boolean(None) => true,
            OracleValue::Varchar2(s) => s.is_empty(),
            _ => false,
        }
    }

    /// Evaluates whether this value satisfies an IF/WHILE condition (strictly TRUE; NULL and FALSE are not truthy).
    pub fn is_truthy(&self) -> bool {
        matches!(self, OracleValue::Boolean(Some(true)))
    }

    /// Converts to a string representation for display or SQL binds.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OracleValue::Varchar2(s) => Some(s.as_str()),
            OracleValue::Date(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Coerces value to a floating point number if possible.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            OracleValue::Number(n) => Some(*n),
            OracleValue::Varchar2(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    /// Converts an AST literal to an OracleValue.
    pub fn from_literal(lit: &LiteralValue) -> Self {
        match lit {
            LiteralValue::Null => OracleValue::Null,
            LiteralValue::String(s) => OracleValue::varchar2(s),
            LiteralValue::Number(n) => {
                if let Ok(num) = n.parse::<f64>() {
                    OracleValue::Number(num)
                } else {
                    OracleValue::Null
                }
            }
            LiteralValue::Boolean(b) => OracleValue::Boolean(Some(*b)),
        }
    }

    /// Converts to database driver SqlValue.
    pub fn to_sql_value(&self) -> SqlValue {
        match self {
            OracleValue::Null => SqlValue::Null,
            OracleValue::Number(n) => SqlValue::Number(n.to_string()),
            OracleValue::Varchar2(s) => SqlValue::Text(s.clone()),
            OracleValue::Date(d) => SqlValue::Date(d.clone()),
            OracleValue::Boolean(Some(b)) => SqlValue::Boolean(*b),
            OracleValue::Boolean(None) => SqlValue::Null,
        }
    }

    /// Converts from database driver SqlValue.
    pub fn from_sql_value(sv: &SqlValue) -> Self {
        match sv {
            SqlValue::Null => OracleValue::Null,
            SqlValue::Text(s) => OracleValue::varchar2(s),
            SqlValue::Number(n) => {
                if let Ok(num) = n.parse::<f64>() {
                    OracleValue::Number(num)
                } else {
                    OracleValue::varchar2(n)
                }
            }
            SqlValue::Date(d) => OracleValue::Date(d.clone()),
            SqlValue::Boolean(b) => OracleValue::Boolean(Some(*b)),
        }
    }

    /// Converts to serde_json::Value for JSON-RPC/REST transport.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            OracleValue::Null => serde_json::Value::Null,
            OracleValue::Number(n) => serde_json::json!(n),
            OracleValue::Varchar2(s) => serde_json::Value::String(s.clone()),
            OracleValue::Date(d) => serde_json::Value::String(d.clone()),
            OracleValue::Boolean(Some(b)) => serde_json::Value::Bool(*b),
            OracleValue::Boolean(None) => serde_json::Value::Null,
        }
    }

    /// Converts from serde_json::Value.
    pub fn from_json(val: &serde_json::Value) -> Self {
        match val {
            serde_json::Value::Null => OracleValue::Null,
            serde_json::Value::Bool(b) => OracleValue::Boolean(Some(*b)),
            serde_json::Value::Number(num) => {
                if let Some(f) = num.as_f64() {
                    OracleValue::Number(f)
                } else {
                    OracleValue::Null
                }
            }
            serde_json::Value::String(s) => OracleValue::varchar2(s),
            _ => OracleValue::varchar2(val.to_string()),
        }
    }

    /// Evaluates binary operator following 3-valued Oracle logic.
    pub fn eval_binary_op(&self, op: BinaryOp, rhs: &OracleValue) -> OracleValue {
        match op {
            // Arithmetic operators: any NULL input yields NULL
            BinaryOp::Add => match (self.as_number(), rhs.as_number()) {
                (Some(a), Some(b)) => OracleValue::Number(a + b),
                _ => OracleValue::Null,
            },
            BinaryOp::Subtract => match (self.as_number(), rhs.as_number()) {
                (Some(a), Some(b)) => OracleValue::Number(a - b),
                _ => OracleValue::Null,
            },
            BinaryOp::Multiply => match (self.as_number(), rhs.as_number()) {
                (Some(a), Some(b)) => OracleValue::Number(a * b),
                _ => OracleValue::Null,
            },
            BinaryOp::Divide => match (self.as_number(), rhs.as_number()) {
                (Some(a), Some(b)) => {
                    if b == 0.0 {
                        OracleValue::Null
                    } else {
                        OracleValue::Number(a / b)
                    }
                }
                _ => OracleValue::Null,
            },
            // String Concatenation: in Oracle, NULL || 'text' is 'text'
            BinaryOp::Concat => {
                let s1 = match self {
                    OracleValue::Null => "",
                    OracleValue::Varchar2(s) => s.as_str(),
                    OracleValue::Number(n) => &n.to_string(),
                    OracleValue::Date(d) => d.as_str(),
                    OracleValue::Boolean(Some(b)) => if *b { "TRUE" } else { "FALSE" },
                    OracleValue::Boolean(None) => "",
                };
                let s2 = match rhs {
                    OracleValue::Null => "",
                    OracleValue::Varchar2(s) => s.as_str(),
                    OracleValue::Number(n) => &n.to_string(),
                    OracleValue::Date(d) => d.as_str(),
                    OracleValue::Boolean(Some(b)) => if *b { "TRUE" } else { "FALSE" },
                    OracleValue::Boolean(None) => "",
                };
                let combined = format!("{}{}", s1, s2);
                OracleValue::varchar2(combined)
            }
            // Equality / Comparison: any NULL operand yields NULL (unknown in 3-valued logic)
            BinaryOp::Equal => {
                if self.is_null() || rhs.is_null() {
                    OracleValue::Boolean(None)
                } else if let (Some(a), Some(b)) = (self.as_number(), rhs.as_number()) {
                    OracleValue::Boolean(Some((a - b).abs() < f64::EPSILON))
                } else {
                    let s1 = self.to_string_lossy();
                    let s2 = rhs.to_string_lossy();
                    OracleValue::Boolean(Some(s1.eq_ignore_ascii_case(&s2)))
                }
            }
            BinaryOp::NotEqual => {
                if self.is_null() || rhs.is_null() {
                    OracleValue::Boolean(None)
                } else if let (Some(a), Some(b)) = (self.as_number(), rhs.as_number()) {
                    OracleValue::Boolean(Some((a - b).abs() >= f64::EPSILON))
                } else {
                    let s1 = self.to_string_lossy();
                    let s2 = rhs.to_string_lossy();
                    OracleValue::Boolean(Some(!s1.eq_ignore_ascii_case(&s2)))
                }
            }
            BinaryOp::LessThan => {
                if self.is_null() || rhs.is_null() {
                    OracleValue::Boolean(None)
                } else if let (Some(a), Some(b)) = (self.as_number(), rhs.as_number()) {
                    OracleValue::Boolean(Some(a < b))
                } else {
                    OracleValue::Boolean(Some(self.to_string_lossy() < rhs.to_string_lossy()))
                }
            }
            BinaryOp::LessThanOrEqual => {
                if self.is_null() || rhs.is_null() {
                    OracleValue::Boolean(None)
                } else if let (Some(a), Some(b)) = (self.as_number(), rhs.as_number()) {
                    OracleValue::Boolean(Some(a <= b))
                } else {
                    OracleValue::Boolean(Some(self.to_string_lossy() <= rhs.to_string_lossy()))
                }
            }
            BinaryOp::GreaterThan => {
                if self.is_null() || rhs.is_null() {
                    OracleValue::Boolean(None)
                } else if let (Some(a), Some(b)) = (self.as_number(), rhs.as_number()) {
                    OracleValue::Boolean(Some(a > b))
                } else {
                    OracleValue::Boolean(Some(self.to_string_lossy() > rhs.to_string_lossy()))
                }
            }
            BinaryOp::GreaterThanOrEqual => {
                if self.is_null() || rhs.is_null() {
                    OracleValue::Boolean(None)
                } else if let (Some(a), Some(b)) = (self.as_number(), rhs.as_number()) {
                    OracleValue::Boolean(Some(a >= b))
                } else {
                    OracleValue::Boolean(Some(self.to_string_lossy() >= rhs.to_string_lossy()))
                }
            }
            // 3-Valued Boolean AND
            BinaryOp::And => {
                let b1 = match self {
                    OracleValue::Boolean(b) => *b,
                    _ => None,
                };
                let b2 = match rhs {
                    OracleValue::Boolean(b) => *b,
                    _ => None,
                };
                match (b1, b2) {
                    (Some(false), _) | (_, Some(false)) => OracleValue::Boolean(Some(false)),
                    (Some(true), Some(true)) => OracleValue::Boolean(Some(true)),
                    _ => OracleValue::Boolean(None),
                }
            }
            // 3-Valued Boolean OR
            BinaryOp::Or => {
                let b1 = match self {
                    OracleValue::Boolean(b) => *b,
                    _ => None,
                };
                let b2 = match rhs {
                    OracleValue::Boolean(b) => *b,
                    _ => None,
                };
                match (b1, b2) {
                    (Some(true), _) | (_, Some(true)) => OracleValue::Boolean(Some(true)),
                    (Some(false), Some(false)) => OracleValue::Boolean(Some(false)),
                    _ => OracleValue::Boolean(None),
                }
            }
            // LIKE operator (supports % and _)
            BinaryOp::Like => {
                if self.is_null() || rhs.is_null() {
                    OracleValue::Boolean(None)
                } else {
                    let s = self.to_string_lossy();
                    let pattern = rhs.to_string_lossy();
                    let regex_pattern = format!(
                        "^{}$",
                        regex::escape(&pattern)
                            .replace("%", ".*")
                            .replace("_", ".")
                    );
                    if let Ok(re) = regex::Regex::new(&format!("(?i){}", regex_pattern)) {
                        OracleValue::Boolean(Some(re.is_match(&s)))
                    } else {
                        OracleValue::Boolean(Some(false))
                    }
                }
            }
        }
    }

    /// Evaluates unary operator.
    pub fn eval_unary_op(&self, op: UnaryOp) -> OracleValue {
        match op {
            UnaryOp::Not => match self {
                OracleValue::Boolean(Some(b)) => OracleValue::Boolean(Some(!*b)),
                _ => OracleValue::Boolean(None),
            },
            UnaryOp::Negate => match self.as_number() {
                Some(n) => OracleValue::Number(-n),
                None => OracleValue::Null,
            },
        }
    }

    fn to_string_lossy(&self) -> String {
        match self {
            OracleValue::Null => String::new(),
            OracleValue::Number(n) => n.to_string(),
            OracleValue::Varchar2(s) => s.clone(),
            OracleValue::Date(d) => d.clone(),
            OracleValue::Boolean(Some(true)) => "TRUE".to_string(),
            OracleValue::Boolean(Some(false)) => "FALSE".to_string(),
            OracleValue::Boolean(None) => String::new(),
        }
    }
}
