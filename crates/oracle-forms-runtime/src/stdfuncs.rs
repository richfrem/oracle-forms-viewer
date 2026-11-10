//! Standard SQL/PL/SQL functions evaluated natively (no database round trip).
//!
//! Functions whose exact behaviour depends on NLS settings or format models
//! (`TO_CHAR` with a format, `TO_DATE`) are only handled in their simple forms here;
//! anything else falls through to the database.

use crate::value::{format_number, parse_number, OracleValue, ValueError};
use bigdecimal::{BigDecimal, RoundingMode, Signed, Zero};

fn text(v: &OracleValue) -> Option<String> {
    v.as_text()
}

fn num(v: &OracleValue) -> Result<Option<BigDecimal>, ValueError> {
    if v.is_null() {
        return Ok(None);
    }
    v.as_decimal().map(Some).ok_or_else(|| ValueError {
        exception: "VALUE_ERROR",
        message: format!("ORA-06502: numeric or value error: '{}' is not a number", v.to_string_lossy()),
    })
}

fn int_arg(args: &[OracleValue], i: usize) -> Result<Option<i64>, ValueError> {
    Ok(match args.get(i) {
        None => None,
        Some(v) => num(v)?.map(|d| {
            use bigdecimal::ToPrimitive;
            d.with_scale_round(0, RoundingMode::Down).to_i64().unwrap_or(0)
        }),
    })
}

/// 1-based, negative-from-end SUBSTR semantics.
fn substr(s: &str, start: i64, len: Option<i64>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len() as i64;
    let begin = if start > 0 {
        start - 1
    } else if start < 0 {
        n + start
    } else {
        0
    };
    if begin < 0 || begin >= n {
        return String::new();
    }
    let end = match len {
        Some(l) if l < 1 => return String::new(),
        Some(l) => (begin + l).min(n),
        None => n,
    };
    chars[begin as usize..end as usize].iter().collect()
}

fn pad(s: &str, width: i64, fill: &str, left: bool) -> String {
    if width <= 0 {
        return String::new();
    }
    let width = width as usize;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() >= width {
        return chars[..width].iter().collect();
    }
    let fill: Vec<char> = if fill.is_empty() { vec![' '] } else { fill.chars().collect() };
    let padding: String = fill.iter().cycle().take(width - chars.len()).collect();
    if left {
        format!("{}{}", padding, s)
    } else {
        format!("{}{}", s, padding)
    }
}

fn trim_chars(s: &str, set: &str, left: bool, right: bool) -> String {
    let set: Vec<char> = if set.is_empty() { vec![' '] } else { set.chars().collect() };
    let mut out: &str = s;
    if left {
        out = out.trim_start_matches(|c| set.contains(&c));
    }
    if right {
        out = out.trim_end_matches(|c| set.contains(&c));
    }
    out.to_string()
}

fn compare_all(args: &[OracleValue], want_greatest: bool) -> Result<OracleValue, ValueError> {
    use oracle_forms_plsql::ast::BinaryOp;
    if args.iter().any(|a| a.is_null()) {
        return Ok(OracleValue::Null);
    }
    let mut best = args[0].clone();
    for a in &args[1..] {
        let op = if want_greatest { BinaryOp::GreaterThan } else { BinaryOp::LessThan };
        if a.eval_binary_op(op, &best)?.is_truthy() {
            best = a.clone();
        }
    }
    Ok(best)
}

/// Evaluates a standard function, or `None` if `name` is not one handled natively.
pub fn call(name: &str, args: &[OracleValue]) -> Option<Result<OracleValue, ValueError>> {
    let a = |i: usize| args.get(i).cloned().unwrap_or(OracleValue::Null);
    let res: Result<OracleValue, ValueError> = (|| {
        Ok(match name {
            "NVL" if args.len() == 2 => if a(0).is_null() { a(1) } else { a(0) },
            "NVL2" if args.len() == 3 => if a(0).is_null() { a(2) } else { a(1) },
            "COALESCE" if !args.is_empty() => args.iter().find(|v| !v.is_null()).cloned().unwrap_or(OracleValue::Null),
            "NULLIF" if args.len() == 2 => {
                if a(0).eval_binary_op(oracle_forms_plsql::ast::BinaryOp::Equal, &a(1))?.is_truthy() { OracleValue::Null } else { a(0) }
            }
            "DECODE" if args.len() >= 3 => {
                // DECODE treats NULL = NULL as a match.
                let subject = a(0);
                let mut i = 1;
                while i + 1 < args.len() {
                    let candidate = &args[i];
                    let matched = (subject.is_null() && candidate.is_null())
                        || subject.eval_binary_op(oracle_forms_plsql::ast::BinaryOp::Equal, candidate)?.is_truthy();
                    if matched {
                        return Ok(args[i + 1].clone());
                    }
                    i += 2;
                }
                if i < args.len() { args[i].clone() } else { OracleValue::Null }
            }
            "UPPER" => text(&a(0)).map(|s| OracleValue::varchar2(s.to_uppercase())).unwrap_or_default(),
            "LOWER" => text(&a(0)).map(|s| OracleValue::varchar2(s.to_lowercase())).unwrap_or_default(),
            "INITCAP" => text(&a(0))
                .map(|s| {
                    let mut out = String::new();
                    let mut start = true;
                    for c in s.chars() {
                        if c.is_alphanumeric() {
                            out.extend(if start { c.to_uppercase().collect::<Vec<_>>() } else { c.to_lowercase().collect() });
                            start = false;
                        } else {
                            out.push(c);
                            start = true;
                        }
                    }
                    OracleValue::varchar2(out)
                })
                .unwrap_or_default(),
            "LENGTH" => text(&a(0)).map(|s| OracleValue::int(s.chars().count() as i64)).unwrap_or_default(),
            "SUBSTR" if args.len() >= 2 => match (text(&a(0)), int_arg(args, 1)?) {
                (Some(s), Some(start)) => OracleValue::varchar2(substr(&s, start, int_arg(args, 2)?)),
                _ => OracleValue::Null,
            },
            "INSTR" if args.len() >= 2 => match (text(&a(0)), text(&a(1))) {
                (Some(s), Some(sub)) => {
                    let start = int_arg(args, 2)?.unwrap_or(1).max(1) as usize;
                    let hay: Vec<char> = s.chars().collect();
                    let needle: Vec<char> = sub.chars().collect();
                    let pos = (start - 1..hay.len().saturating_sub(needle.len()) + 1)
                        .find(|&i| hay[i..].starts_with(&needle))
                        .map(|i| i as i64 + 1)
                        .unwrap_or(0);
                    OracleValue::int(pos)
                }
                _ => OracleValue::Null,
            },
            "TRIM" => text(&a(0)).map(|s| OracleValue::varchar2(s.trim_matches(' ').to_string())).unwrap_or_default(),
            "LTRIM" => text(&a(0)).map(|s| OracleValue::varchar2(trim_chars(&s, &text(&a(1)).unwrap_or_default(), true, false))).unwrap_or_default(),
            "RTRIM" => text(&a(0)).map(|s| OracleValue::varchar2(trim_chars(&s, &text(&a(1)).unwrap_or_default(), false, true))).unwrap_or_default(),
            "LPAD" | "RPAD" if args.len() >= 2 => match (text(&a(0)), int_arg(args, 1)?) {
                (Some(s), Some(w)) => OracleValue::varchar2(pad(&s, w, &text(&a(2)).unwrap_or_else(|| " ".into()), name == "LPAD")),
                _ => OracleValue::Null,
            },
            "REPLACE" if args.len() >= 2 => match text(&a(0)) {
                Some(s) => {
                    let from = text(&a(1)).unwrap_or_default();
                    let to = text(&a(2)).unwrap_or_default();
                    if from.is_empty() { OracleValue::varchar2(s) } else { OracleValue::varchar2(s.replace(&from, &to)) }
                }
                None => OracleValue::Null,
            },
            "CONCAT" if args.len() == 2 => OracleValue::varchar2(format!("{}{}", a(0).to_string_lossy(), a(1).to_string_lossy())),
            "CHR" => match int_arg(args, 0)? {
                Some(n) => char::from_u32(n as u32).map(|c| OracleValue::varchar2(c.to_string())).unwrap_or_default(),
                None => OracleValue::Null,
            },
            "ASCII" => text(&a(0)).and_then(|s| s.chars().next()).map(|c| OracleValue::int(c as i64)).unwrap_or_default(),
            "TO_NUMBER" if args.len() == 1 => match &a(0) {
                v if v.is_null() => OracleValue::Null,
                OracleValue::Number(_) => a(0),
                v => match parse_number(&v.to_string_lossy()) {
                    Some(n) => OracleValue::Number(n),
                    None => return Err(ValueError { exception: "VALUE_ERROR", message: format!("ORA-01722: invalid number '{}'", v.to_string_lossy()) }),
                },
            },
            // TO_CHAR without a format model only; formatted conversions go to the database.
            "TO_CHAR" if args.len() == 1 => match &a(0) {
                v if v.is_null() => OracleValue::Null,
                OracleValue::Number(n) => OracleValue::varchar2(format_number(n)),
                v => OracleValue::varchar2(v.to_string_lossy()),
            },
            "ABS" => num(&a(0))?.map(|n| OracleValue::Number(n.abs())).unwrap_or_default(),
            "SIGN" => num(&a(0))?.map(|n| OracleValue::int(if n.is_zero() { 0 } else if n.is_negative() { -1 } else { 1 })).unwrap_or_default(),
            "CEIL" => num(&a(0))?.map(|n| OracleValue::Number(n.with_scale_round(0, RoundingMode::Ceiling))).unwrap_or_default(),
            "FLOOR" => num(&a(0))?.map(|n| OracleValue::Number(n.with_scale_round(0, RoundingMode::Floor))).unwrap_or_default(),
            "ROUND" if !matches!(a(0), OracleValue::Date(_)) => match num(&a(0))? {
                Some(n) => OracleValue::Number(n.with_scale_round(int_arg(args, 1)?.unwrap_or(0), RoundingMode::HalfUp)),
                None => OracleValue::Null,
            },
            "TRUNC" if !matches!(a(0), OracleValue::Date(_)) => match num(&a(0))? {
                Some(n) => OracleValue::Number(n.with_scale_round(int_arg(args, 1)?.unwrap_or(0), RoundingMode::Down)),
                None => OracleValue::Null,
            },
            "MOD" if args.len() == 2 => match (num(&a(0))?, num(&a(1))?) {
                (Some(x), Some(y)) if y.is_zero() => OracleValue::Number(x),
                (Some(x), Some(y)) => OracleValue::Number(x.clone() - (x / y.clone()).with_scale_round(0, RoundingMode::Down) * y),
                _ => OracleValue::Null,
            },
            "POWER" if args.len() == 2 => match (num(&a(0))?, int_arg(args, 1)?) {
                (Some(x), Some(e)) if (0..=1000).contains(&e) => {
                    let mut r = BigDecimal::from(1);
                    for _ in 0..e {
                        r *= x.clone();
                    }
                    OracleValue::Number(r)
                }
                _ => return Err(ValueError { exception: "VALUE_ERROR", message: "POWER supports non-negative integer exponents here".into() }),
            },
            "GREATEST" if !args.is_empty() => compare_all(args, true)?,
            "LEAST" if !args.is_empty() => compare_all(args, false)?,
            "SYSDATE" | "CURRENT_DATE" if args.is_empty() => OracleValue::Date(now_iso()),
            "SYSTIMESTAMP" | "CURRENT_TIMESTAMP" if args.is_empty() => OracleValue::Date(now_iso()),
            _ => return Err(ValueError { exception: "__NOT_A_STDFUNC__", message: String::new() }),
        })
    })();
    match res {
        Err(ValueError { exception: "__NOT_A_STDFUNC__", .. }) => None,
        other => Some(other),
    }
}

/// Current UTC time as `YYYY-MM-DD HH:MM:SS` (no timezone database dependency).
fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> OracleValue {
        OracleValue::varchar2(v)
    }
    fn n(v: &str) -> OracleValue {
        OracleValue::number_from_str(v)
    }
    fn c(name: &str, args: &[OracleValue]) -> OracleValue {
        call(name, args).expect("is a std function").expect("no error")
    }

    #[test]
    fn common_functions() {
        assert_eq!(c("NVL", &[OracleValue::Null, s("x")]), s("x"));
        assert_eq!(c("DECODE", &[OracleValue::Null, OracleValue::Null, s("null-match"), s("other")]), s("null-match"));
        assert_eq!(c("SUBSTR", &[s("ORACLE"), n("2"), n("3")]), s("RAC"));
        assert_eq!(c("SUBSTR", &[s("ORACLE"), n("-3")]), s("CLE"));
        assert_eq!(c("INSTR", &[s("FORMS"), s("RM")]), n("3"));
        assert_eq!(c("LPAD", &[s("7"), n("3"), s("0")]), s("007"));
        assert_eq!(c("ROUND", &[n("2.345"), n("2")]), n("2.35"));
        assert_eq!(c("TRUNC", &[n("2.999")]), n("2"));
        assert_eq!(c("MOD", &[n("11"), n("4")]), n("3"));
        assert_eq!(c("INITCAP", &[s("hello wORLD")]), s("Hello World"));
        assert_eq!(c("TO_CHAR", &[n("0.5")]), s(".5"));
        assert_eq!(c("GREATEST", &[n("1"), n("7"), n("3")]), n("7"));
        assert!(call("NOT_A_FUNCTION", &[]).is_none());
        assert_eq!(call("TO_NUMBER", &[s("12x")]).unwrap().unwrap_err().exception, "VALUE_ERROR");
    }
}
