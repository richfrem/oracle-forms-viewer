//! Oracle value semantics for the Forms PL/SQL interpreter.
//!
//! Matches Oracle where it matters for business logic:
//! * `NUMBER` is an exact decimal (no binary floating point), up to 38 significant digits.
//! * `''` is `NULL` for character data.
//! * Comparisons with `NULL` are unknown (`NULL`), and `IF` only takes the branch on TRUE.
//! * Character comparison and `LIKE` are case-sensitive.
//! * Mixing a number and a string converts the string with `TO_NUMBER`; an invalid
//!   string raises `VALUE_ERROR`, and division by zero raises `ZERO_DIVIDE`.

use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use oracle_forms_db::model::SqlValue;
use oracle_forms_plsql::ast::{BinaryOp, LiteralValue, UnaryOp};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Significant digits kept by Oracle `NUMBER`.
const NUMBER_PRECISION: u64 = 38;

/// Oracle runtime value supporting 3-valued logic and Oracle NULL semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum OracleValue {
    #[default]
    Null,
    Number(BigDecimal),
    Varchar2(String),
    Date(String),
    Boolean(Option<bool>), // Some(true), Some(false), None (NULL)
}

/// A predefined PL/SQL exception raised while evaluating an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueError {
    /// Exception name, e.g. `VALUE_ERROR`, `ZERO_DIVIDE`.
    pub exception: &'static str,
    pub message: String,
}

impl ValueError {
    fn invalid(message: impl Into<String>) -> Self {
        Self { exception: "VALUE_ERROR", message: message.into() }
    }
}

/// Parses Oracle numeric text (`12`, `-3.50`, `.5`, `1e3`) into a decimal.
pub fn parse_number(text: &str) -> Option<BigDecimal> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    BigDecimal::from_str(t).ok()
}

/// Formats a decimal the way Oracle's implicit `TO_CHAR` does: no trailing zeros, and no
/// leading zero before the decimal point (`0.5` → `.5`, `-0.5` → `-.5`).
pub fn format_number(n: &BigDecimal) -> String {
    if n.is_zero() {
        return "0".to_string();
    }
    let mut s = n.normalized().to_plain_string();
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if let Some(rest) = s.strip_prefix("0.") {
        s = format!(".{}", rest);
    } else if let Some(rest) = s.strip_prefix("-0.") {
        s = format!("-.{}", rest);
    }
    s
}

fn round_to_oracle_precision(n: BigDecimal) -> BigDecimal {
    n.with_prec(NUMBER_PRECISION)
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

    /// Number from an `f64`. Prefer [`OracleValue::number_from_str`] for exact values.
    pub fn number(n: f64) -> Self {
        BigDecimal::from_str(&n.to_string()).map(OracleValue::Number).unwrap_or(OracleValue::Null)
    }

    pub fn int(n: i64) -> Self {
        OracleValue::Number(BigDecimal::from(n))
    }

    pub fn number_from_str(s: &str) -> Self {
        parse_number(s).map(OracleValue::Number).unwrap_or(OracleValue::Null)
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

    /// Borrowed text for character and date values.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OracleValue::Varchar2(s) => Some(s.as_str()),
            OracleValue::Date(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Text form of any non-NULL value (numbers formatted as Oracle does).
    pub fn as_text(&self) -> Option<String> {
        if self.is_null() {
            return None;
        }
        Some(self.to_string_lossy())
    }

    /// Numeric value, converting character data like `TO_NUMBER` would. `None` for NULL
    /// or non-numeric text.
    pub fn as_decimal(&self) -> Option<BigDecimal> {
        match self {
            OracleValue::Number(n) => Some(n.clone()),
            OracleValue::Varchar2(s) => parse_number(s),
            _ => None,
        }
    }

    /// Numeric value as `f64` (lossy; for loop bounds and display only).
    pub fn as_number(&self) -> Option<f64> {
        self.as_decimal().and_then(|d| d.to_f64())
    }

    /// Numeric value as `i64` (truncating), for loop bounds and built-in arguments.
    pub fn as_i64(&self) -> Option<i64> {
        self.as_decimal().and_then(|d| d.with_scale(0).to_i64())
    }

    /// Strict numeric conversion: NULL stays NULL, invalid text raises `VALUE_ERROR`.
    fn to_decimal_strict(&self) -> Result<Option<BigDecimal>, ValueError> {
        match self {
            v if v.is_null() => Ok(None),
            OracleValue::Number(n) => Ok(Some(n.clone())),
            OracleValue::Varchar2(s) => parse_number(s)
                .map(Some)
                .ok_or_else(|| ValueError::invalid(format!("ORA-06502: character to number conversion error ('{}')", s))),
            OracleValue::Date(d) => Err(ValueError::invalid(format!("ORA-06502: cannot use DATE '{}' as a number", d))),
            OracleValue::Boolean(_) => Err(ValueError::invalid("PLS-00306: BOOLEAN used as a number")),
            OracleValue::Null => Ok(None),
        }
    }

    /// Converts an AST literal to an OracleValue.
    pub fn from_literal(lit: &LiteralValue) -> Self {
        match lit {
            LiteralValue::Null => OracleValue::Null,
            LiteralValue::String(s) => OracleValue::varchar2(s),
            LiteralValue::Number(n) => OracleValue::number_from_str(n),
            LiteralValue::Boolean(b) => OracleValue::Boolean(Some(*b)),
        }
    }

    /// Converts to database driver SqlValue.
    pub fn to_sql_value(&self) -> SqlValue {
        match self {
            OracleValue::Null => SqlValue::Null,
            OracleValue::Number(n) => SqlValue::Number(n.normalized().to_plain_string()),
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
            SqlValue::Number(n) => parse_number(n).map(OracleValue::Number).unwrap_or_else(|| OracleValue::varchar2(n)),
            SqlValue::Date(d) => OracleValue::Date(d.clone()),
            SqlValue::Boolean(b) => OracleValue::Boolean(Some(*b)),
        }
    }

    /// Converts to serde_json::Value for JSON-RPC/REST transport. Numbers that cannot be
    /// represented exactly as a JSON (f64) number are sent as strings.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            OracleValue::Null => serde_json::Value::Null,
            OracleValue::Number(n) => {
                if let Some(i) = n.to_i64().filter(|i| BigDecimal::from(*i) == *n) {
                    return serde_json::Value::from(i);
                }
                let text = n.normalized().to_plain_string();
                match text.parse::<f64>() {
                    Ok(f) if BigDecimal::from_str(&f.to_string()).ok().as_ref() == Some(&n.normalized()) => {
                        serde_json::Number::from_f64(f).map(serde_json::Value::Number).unwrap_or(serde_json::Value::String(text))
                    }
                    _ => serde_json::Value::String(text),
                }
            }
            OracleValue::Varchar2(s) => serde_json::Value::String(s.clone()),
            OracleValue::Date(d) => serde_json::Value::String(d.clone()),
            OracleValue::Boolean(Some(b)) => serde_json::Value::Bool(*b),
            OracleValue::Boolean(None) => serde_json::Value::Null,
        }
    }

    /// Converts from serde_json::Value. JSON numbers use their exact text.
    pub fn from_json(val: &serde_json::Value) -> Self {
        match val {
            serde_json::Value::Null => OracleValue::Null,
            serde_json::Value::Bool(b) => OracleValue::Boolean(Some(*b)),
            serde_json::Value::Number(num) => OracleValue::number_from_str(&num.to_string()),
            serde_json::Value::String(s) => OracleValue::varchar2(s),
            _ => OracleValue::varchar2(val.to_string()),
        }
    }

    /// Compares two non-NULL values the way Oracle does. If either side is a number the
    /// comparison is numeric (converting the other side, which may raise `VALUE_ERROR`);
    /// otherwise it is a case-sensitive string comparison.
    fn compare(&self, rhs: &OracleValue) -> Result<Option<std::cmp::Ordering>, ValueError> {
        if self.is_null() || rhs.is_null() {
            return Ok(None);
        }
        let numeric = matches!(self, OracleValue::Number(_)) || matches!(rhs, OracleValue::Number(_));
        if numeric {
            let a = self.to_decimal_strict()?.expect("non-null");
            let b = rhs.to_decimal_strict()?.expect("non-null");
            return Ok(Some(a.cmp(&b)));
        }
        if let (OracleValue::Boolean(Some(a)), OracleValue::Boolean(Some(b))) = (self, rhs) {
            return Ok(Some(a.cmp(b)));
        }
        Ok(Some(self.to_string_lossy().cmp(&rhs.to_string_lossy())))
    }

    fn as_tribool(&self) -> Option<bool> {
        match self {
            OracleValue::Boolean(b) => *b,
            _ => None,
        }
    }

    /// Evaluates a binary operator with Oracle semantics.
    pub fn eval_binary_op(&self, op: BinaryOp, rhs: &OracleValue) -> Result<OracleValue, ValueError> {
        use std::cmp::Ordering::*;
        let arith = |f: &dyn Fn(BigDecimal, BigDecimal) -> Result<BigDecimal, ValueError>| -> Result<OracleValue, ValueError> {
            match (self.to_decimal_strict()?, rhs.to_decimal_strict()?) {
                (Some(a), Some(b)) => Ok(OracleValue::Number(round_to_oracle_precision(f(a, b)?))),
                _ => Ok(OracleValue::Null),
            }
        };
        let cmp = |pred: &dyn Fn(std::cmp::Ordering) -> bool| -> Result<OracleValue, ValueError> {
            Ok(OracleValue::Boolean(self.compare(rhs)?.map(pred)))
        };

        match op {
            BinaryOp::Add => arith(&|a, b| Ok(a + b)),
            BinaryOp::Subtract => arith(&|a, b| Ok(a - b)),
            BinaryOp::Multiply => arith(&|a, b| Ok(a * b)),
            BinaryOp::Divide => arith(&|a, b| {
                if b.is_zero() {
                    Err(ValueError { exception: "ZERO_DIVIDE", message: "ORA-01476: divisor is equal to zero".to_string() })
                } else {
                    Ok(a / b)
                }
            }),
            // NULL || 'text' is 'text' in Oracle.
            BinaryOp::Concat => Ok(OracleValue::varchar2(format!("{}{}", self.to_string_lossy(), rhs.to_string_lossy()))),
            BinaryOp::Equal => cmp(&|o| o == Equal),
            BinaryOp::NotEqual => cmp(&|o| o != Equal),
            BinaryOp::LessThan => cmp(&|o| o == Less),
            BinaryOp::LessThanOrEqual => cmp(&|o| o != Greater),
            BinaryOp::GreaterThan => cmp(&|o| o == Greater),
            BinaryOp::GreaterThanOrEqual => cmp(&|o| o != Less),
            BinaryOp::And => Ok(OracleValue::Boolean(match (self.as_tribool(), rhs.as_tribool()) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            })),
            BinaryOp::Or => Ok(OracleValue::Boolean(match (self.as_tribool(), rhs.as_tribool()) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            })),
            BinaryOp::Like => {
                if self.is_null() || rhs.is_null() {
                    return Ok(OracleValue::Boolean(None));
                }
                Ok(OracleValue::Boolean(Some(like_match(&self.to_string_lossy(), &rhs.to_string_lossy()))))
            }
        }
    }

    /// Evaluates a unary operator.
    pub fn eval_unary_op(&self, op: UnaryOp) -> Result<OracleValue, ValueError> {
        Ok(match op {
            UnaryOp::Not => match self {
                OracleValue::Boolean(Some(b)) => OracleValue::Boolean(Some(!*b)),
                _ => OracleValue::Boolean(None),
            },
            UnaryOp::Negate => match self.to_decimal_strict()? {
                Some(n) => OracleValue::Number(-n),
                None => OracleValue::Null,
            },
            UnaryOp::IsNull => OracleValue::Boolean(Some(self.is_null())),
            UnaryOp::IsNotNull => OracleValue::Boolean(Some(!self.is_null())),
        })
    }

    /// Text used for concatenation and string comparison.
    pub fn to_string_lossy(&self) -> String {
        match self {
            OracleValue::Null => String::new(),
            OracleValue::Number(n) => format_number(n),
            OracleValue::Varchar2(s) => s.clone(),
            OracleValue::Date(d) => d.clone(),
            OracleValue::Boolean(Some(true)) => "TRUE".to_string(),
            OracleValue::Boolean(Some(false)) => "FALSE".to_string(),
            OracleValue::Boolean(None) => String::new(),
        }
    }
}

/// True if `s` looks like an ISO date or date-time (`YYYY-MM-DD[ HH:MM[:SS[.f]]]`).
pub fn looks_like_date(s: &str) -> bool {
    let b = s.trim().as_bytes();
    b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && (b.len() == 10 || b[10] == b' ' || b[10] == b'T')
}

impl OracleValue {
    /// Converts a value to a declared PL/SQL / Forms data type (`DATE`, `NUMBER`,
    /// `VARCHAR2`, ...), as an assignment would. Unknown or anchored types (`%TYPE`) and
    /// values that do not convert are left unchanged.
    pub fn coerce_to(self, data_type: &str) -> OracleValue {
        let t = data_type.trim().to_ascii_uppercase();
        let base = t.split(['(', ' ']).next().unwrap_or("");
        match (base, self) {
            (_, OracleValue::Null) => OracleValue::Null,
            ("DATE" | "DATETIME" | "TIMESTAMP", OracleValue::Varchar2(s)) if looks_like_date(&s) => {
                OracleValue::Date(s.trim().replace('T', " "))
            }
            (
                "NUMBER" | "INTEGER" | "INT" | "PLS_INTEGER" | "BINARY_INTEGER" | "NATURAL" | "POSITIVE" | "DECIMAL"
                | "NUMERIC" | "FLOAT" | "REAL" | "SMALLINT" | "MONEY" | "RNUMBER" | "RINTEGER",
                OracleValue::Varchar2(s),
            ) => parse_number(&s).map(OracleValue::Number).unwrap_or(OracleValue::Varchar2(s)),
            ("VARCHAR2" | "VARCHAR" | "CHAR" | "NVARCHAR2" | "NCHAR" | "LONG" | "ALPHA", v @ (OracleValue::Number(_) | OracleValue::Date(_))) => {
                OracleValue::varchar2(v.to_string_lossy())
            }
            (_, v) => v,
        }
    }
}

/// Case-sensitive SQL `LIKE` with `%` and `_` wildcards.
pub fn like_match(text: &str, pattern: &str) -> bool {
    let t: Vec<char> = text.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    // Iterative wildcard match with backtracking on the last `%`.
    let (mut ti, mut pi) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == '%' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && (p[pi] == '_' || p[pi] == t[ti]) {
            ti += 1;
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '%' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> OracleValue {
        OracleValue::number_from_str(s)
    }
    fn s(v: &str) -> OracleValue {
        OracleValue::varchar2(v)
    }

    #[test]
    fn decimal_arithmetic_is_exact() {
        assert_eq!(n("0.1").eval_binary_op(BinaryOp::Add, &n("0.2")).unwrap(), n("0.3"));
        assert_eq!(n("19.99").eval_binary_op(BinaryOp::Multiply, &n("3")).unwrap(), n("59.97"));
        let big = n("12345678901234567890123456789012345678");
        assert_eq!(big.eval_binary_op(BinaryOp::Subtract, &n("1")).unwrap(), n("12345678901234567890123456789012345677"));
    }

    #[test]
    fn division_by_zero_raises() {
        let e = n("1").eval_binary_op(BinaryOp::Divide, &n("0")).unwrap_err();
        assert_eq!(e.exception, "ZERO_DIVIDE");
    }

    #[test]
    fn string_to_number_conversion() {
        assert_eq!(s("10").eval_binary_op(BinaryOp::Add, &n("5")).unwrap(), n("15"));
        assert_eq!(s("abc").eval_binary_op(BinaryOp::Add, &n("5")).unwrap_err().exception, "VALUE_ERROR");
        // number vs string compares numerically: '010' = 10
        assert!(s("010").eval_binary_op(BinaryOp::Equal, &n("10")).unwrap().is_truthy());
        // two strings compare as strings: '010' <> '10'
        assert!(s("010").eval_binary_op(BinaryOp::NotEqual, &s("10")).unwrap().is_truthy());
    }

    #[test]
    fn string_comparison_and_like_are_case_sensitive() {
        assert!(!s("abc").eval_binary_op(BinaryOp::Equal, &s("ABC")).unwrap().is_truthy());
        assert!(s("SMITH").eval_binary_op(BinaryOp::Like, &s("SM_T%")).unwrap().is_truthy());
        assert!(!s("smith").eval_binary_op(BinaryOp::Like, &s("SM%")).unwrap().is_truthy());
        assert!(like_match("a%b", "a%"));
        assert!(like_match("", "%"));
        assert!(!like_match("abc", "a_"));
    }

    #[test]
    fn null_semantics() {
        assert_eq!(OracleValue::varchar2(""), OracleValue::Null);
        assert_eq!(OracleValue::Null.eval_binary_op(BinaryOp::Equal, &OracleValue::Null).unwrap(), OracleValue::Boolean(None));
        assert!(OracleValue::Null.eval_unary_op(UnaryOp::IsNull).unwrap().is_truthy());
        assert_eq!(OracleValue::Null.eval_binary_op(BinaryOp::Concat, &s("x")).unwrap(), s("x"));
        assert_eq!(OracleValue::Null.eval_binary_op(BinaryOp::Add, &n("1")).unwrap(), OracleValue::Null);
        let f = OracleValue::boolean(false);
        assert_eq!(f.eval_binary_op(BinaryOp::And, &OracleValue::Boolean(None)).unwrap(), f);
    }

    #[test]
    fn number_formatting_matches_oracle() {
        assert_eq!(n("0.5").to_string_lossy(), ".5");
        assert_eq!(n("-0.25").to_string_lossy(), "-.25");
        assert_eq!(n("10.50").to_string_lossy(), "10.5");
        assert_eq!(n("100").to_string_lossy(), "100");
        assert_eq!(n("49.99").to_json(), serde_json::json!(49.99));
        assert_eq!(n("12345678901234567890123456789012345678").to_json(), serde_json::json!("12345678901234567890123456789012345678"));
    }
}
