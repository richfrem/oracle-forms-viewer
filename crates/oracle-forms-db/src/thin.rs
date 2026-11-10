use crate::driver::{validate_savepoint_name, DatabaseDriver};
use crate::model::{BoundSql, DbBackend, DbConfig, DbError, QueryRecord, QueryResult, SqlValue};
use oracledb::{Connection, OracleTimestamp, ToDbValue};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Pure-Rust Oracle thin driver (no Instant Client, no JVM), built on Oracle's `oracledb`
/// crate.
///
/// One driver instance owns **one** database session. Queries, row locks, DML, savepoints
/// and COMMIT all run on that same connection, which is what the Forms commit pipeline
/// needs: a `SELECT … FOR UPDATE NOWAIT` lock taken on first edit must still be held by the
/// session that later runs the UPDATE and the COMMIT.
pub struct OracleThinDriver {
    config: DbConfig,
    conn: Arc<Mutex<Option<Connection>>>,
}

type ConnParams = (String, String, String);

impl OracleThinDriver {
    pub fn new(config: DbConfig) -> Self {
        Self {
            config,
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// Resolves `(user, password, connect_string)` from `DbConfig`. Credentials must be
    /// supplied explicitly (see [`DbConfig::with_env_credentials`]); there are no defaults.
    fn resolve_parameters(&self) -> Result<ConnParams, DbError> {
        let (user, pass) = self.config.require_credentials()?;
        let host = self.config.host.clone().unwrap_or_else(|| "127.0.0.1".to_string());
        let port = self.config.port.unwrap_or(1521);
        let service = self
            .config
            .service_name
            .clone()
            .unwrap_or_else(|| "FREEPDB1".to_string());
        Ok((user, pass, format!("{}:{}/{}", host, port, service)))
    }

    /// Runs `f` on this driver's session connection (opening it on first use) in a
    /// blocking task.
    async fn with_conn<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, DbError> + Send + 'static,
    {
        let params = self.resolve_parameters()?;
        let slot = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut guard = slot
                .lock()
                .map_err(|e| DbError::Internal(format!("Oracle session mutex poisoned: {}", e)))?;
            if guard.is_none() {
                let (user, pass, connect_string) = &params;
                let cfg = oracledb::Config::default()
                    .set_credentials(user, pass)
                    .set_connect_string(connect_string)
                    .map_err(|e| {
                        DbError::ConnectionError(format!(
                            "Invalid Oracle connect string '{}': {}",
                            connect_string, e
                        ))
                    })?;
                let conn = oracledb::connect(cfg).map_err(|e| {
                    DbError::ConnectionError(format!("Failed to connect to Oracle at '{}': {}", connect_string, e))
                })?;
                *guard = Some(conn);
            }
            f(guard.as_ref().expect("connection initialised above"))
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
    }
}

/// Owned bind value with the Rust type `oracledb` should send.
enum OwnedBind {
    Text(Option<String>),
    Int(Option<i64>),
    Float(Option<f64>),
    Timestamp(Option<OracleTimestamp>),
}

/// Parses `YYYY-MM-DD`, `YYYY-MM-DD HH:MM:SS[.f]` or the ISO `T` form.
fn parse_datetime(s: &str) -> Option<chrono::NaiveDateTime> {
    let t = s.trim().replace('T', " ");
    for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&t, fmt) {
            return Some(dt);
        }
    }
    chrono::NaiveDate::parse_from_str(&t, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0))
}

impl OwnedBind {
    fn from_sql(val: &SqlValue) -> Self {
        match val {
            SqlValue::Null => OwnedBind::Text(None),
            SqlValue::Text(s) => OwnedBind::Text(Some(s.clone())),
            // Dates are sent as real Oracle date/time values, never as text that Oracle
            // would have to convert with NLS settings.
            SqlValue::Date(s) => match parse_datetime(s) {
                Some(dt) => OwnedBind::Timestamp(Some(OracleTimestamp::from(dt))),
                None => OwnedBind::Text(Some(s.clone())),
            },
            SqlValue::Number(n) => {
                let t = n.trim();
                if let Ok(i) = t.parse::<i64>() {
                    OwnedBind::Int(Some(i))
                } else if t.parse::<f64>().map(|f| f.to_string() == t).unwrap_or(false) {
                    OwnedBind::Float(t.parse().ok())
                } else {
                    // Keep exact decimal text; Oracle converts it to NUMBER without loss.
                    OwnedBind::Text(Some(t.to_string()))
                }
            }
            SqlValue::Boolean(b) => OwnedBind::Int(Some(i64::from(*b))),
        }
    }

    fn as_db_value(&self) -> &dyn ToDbValue {
        match self {
            OwnedBind::Text(v) => v,
            OwnedBind::Int(v) => v,
            OwnedBind::Float(v) => v,
            OwnedBind::Timestamp(v) => v,
        }
    }
}

/// Converts `BoundSql` binds to named `oracledb` binds. Bind names must be plain
/// identifiers (`:b1`); a dotted Forms reference such as `:BLOCK.ITEM` reaching the driver
/// means the caller forgot to rewrite it, which is reported instead of sent to Oracle.
fn prepare_binds(stmt: &BoundSql) -> Result<Vec<(String, OwnedBind)>, DbError> {
    stmt.binds
        .iter()
        .map(|(name, val)| {
            let clean = name.strip_prefix(':').unwrap_or(name).to_string();
            if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(DbError::QueryError(format!(
                    "Bind name '{}' is not a valid Oracle bind identifier (rewrite :BLOCK.ITEM references first)",
                    name
                )));
            }
            Ok((clean, OwnedBind::from_sql(val)))
        })
        .collect()
}

/// Converts NUMBER text to JSON without losing precision: integers that fit in i64 and
/// decimals that round-trip through f64 become JSON numbers; anything else stays a string.
fn number_to_json(text: &str) -> serde_json::Value {
    let t = text.trim();
    if let Ok(i) = t.parse::<i64>() {
        return serde_json::Value::from(i);
    }
    if let Ok(f) = t.parse::<f64>() {
        let normalized = t.trim_start_matches('+');
        let roundtrips = f.to_string() == normalized
            || format!("0{}", normalized) == f.to_string()
            || normalized.strip_prefix("-.").map(|r| format!("-0.{}", r)) == Some(f.to_string());
        if roundtrips {
            if let Some(n) = serde_json::Number::from_f64(f) {
                return serde_json::Value::Number(n);
            }
        }
    }
    serde_json::Value::String(t.to_string())
}

/// `2016-05-16T00:00:00.000000000` -> `2016-05-16`; otherwise `YYYY-MM-DD HH:MM:SS[.fff]`
/// with trailing zero fractions removed.
fn tidy_datetime(s: &str) -> String {
    let Some((date, time)) = s.split_once('T') else { return s.to_string() };
    let (hms, frac) = time.split_once('.').unwrap_or((time, ""));
    let frac = frac.trim_end_matches('0');
    if hms == "00:00:00" && frac.is_empty() {
        return date.to_string();
    }
    if frac.is_empty() {
        format!("{} {}", date, hms)
    } else {
        format!("{} {}.{}", date, hms, frac)
    }
}

#[async_trait::async_trait]
impl DatabaseDriver for OracleThinDriver {
    async fn test_connection(&self) -> Result<(), DbError> {
        self.with_conn(|conn| {
            let row = conn
                .query_row("SELECT 1 FROM DUAL", &[])
                .map_err(|e| DbError::QueryError(format!("Oracle ping query failed: {}", e)))?;
            let val: i32 = row
                .get(0)
                .map_err(|e| DbError::QueryError(format!("Failed to read ping result: {}", e)))?;
            if val != 1 {
                return Err(DbError::ConnectionError(format!("Unexpected ping response from Oracle: {}", val)));
            }
            Ok(())
        })
        .await
    }

    async fn query(&self, stmt: &BoundSql) -> Result<QueryResult, DbError> {
        let stmt = stmt.clone();
        self.with_conn(move |conn| {
            let start = Instant::now();
            let binds = prepare_binds(&stmt)?;
            let named: Vec<(&str, &dyn ToDbValue)> =
                binds.iter().map(|(k, v)| (k.as_str(), v.as_db_value())).collect();

            let cursor = if named.is_empty() {
                conn.query(&stmt.sql, &[])
            } else {
                conn.query_named(&stmt.sql, &named)
            }
            .map_err(|e| DbError::QueryError(format!("Oracle query execution failed: {}", e)))?;

            let columns: Vec<(String, String)> = cursor
                .columns()
                .iter()
                .map(|c| (c.name().to_string(), c.db_type().name().to_string()))
                .collect();

            let mut records = Vec::new();
            for row_result in cursor {
                let row = row_result.map_err(|e| DbError::QueryError(format!("Failed to fetch Oracle row: {}", e)))?;

                let mut values = HashMap::new();
                let mut row_id_val = None;

                for (idx, (col_name, type_name)) in columns.iter().enumerate() {
                    let upper_type = type_name.to_ascii_uppercase();
                    let json_val = if upper_type.contains("BOOLEAN") {
                        match row.get::<Option<bool>>(idx) {
                            Ok(Some(b)) => serde_json::Value::Bool(b),
                            _ => serde_json::Value::Null,
                        }
                    } else {
                        let text: Option<String> = match row.get::<Option<String>>(idx) {
                            Ok(v) => v,
                            Err(_) => row
                                .get::<Option<OracleTimestamp>>(idx)
                                .ok()
                                .flatten()
                                .map(|ts| ts.to_string()),
                        };
                        match text {
                            None => serde_json::Value::Null,
                            // Only NUMBER-family columns become JSON numbers. VARCHAR2 '00123'
                            // stays the string "00123"; 'true' stays a string.
                            Some(s) if upper_type.contains("NUMBER")
                                || upper_type.contains("BINARY_DOUBLE")
                                || upper_type.contains("BINARY_FLOAT")
                                || upper_type.contains("BINARY_INTEGER") => number_to_json(&s),
                            // DATE / TIMESTAMP: ISO text, date-only when there is no time part.
                            Some(s) if upper_type.contains("DATE") || upper_type.contains("TIMESTAMP") => {
                                serde_json::Value::String(tidy_datetime(&s))
                            }
                            Some(s) => serde_json::Value::String(s),
                        }
                    };

                    let k_upper = col_name.to_ascii_uppercase();
                    if k_upper == "ROWID" || k_upper == "OFW_ROWID" {
                        row_id_val = json_val.as_str().map(str::to_string);
                    }
                    values.insert(k_upper, json_val);
                }

                records.push(QueryRecord { values, row_id: row_id_val });
            }

            let total_count = records.len();
            Ok(QueryResult {
                block_name: String::new(),
                table_name: String::new(),
                columns: columns.into_iter().map(|(n, _)| n).collect(),
                records,
                total_count,
                execution_time_ms: start.elapsed().as_millis() as u64,
                sql_statement: stmt.sql,
            })
        })
        .await
    }

    fn backend(&self) -> DbBackend {
        DbBackend::OracleThin
    }

    fn supports_transactions(&self) -> bool {
        true
    }

    async fn execute(&self, stmt: &BoundSql) -> Result<u64, DbError> {
        let stmt = stmt.clone();
        self.with_conn(move |conn| {
            let binds = prepare_binds(&stmt)?;
            let named: Vec<(&str, &dyn ToDbValue)> =
                binds.iter().map(|(k, v)| (k.as_str(), v.as_db_value())).collect();
            let result = if named.is_empty() {
                conn.execute(&stmt.sql, &[])
            } else {
                conn.execute_named(&stmt.sql, &named)
            }
            .map_err(|e| DbError::QueryError(format!("Oracle DML failed: {}", e)))?;
            Ok(result.rows_affected())
        })
        .await
    }

    async fn savepoint(&self, name: &str) -> Result<(), DbError> {
        validate_savepoint_name(name)?;
        let sql = format!("SAVEPOINT {}", name);
        self.with_conn(move |conn| {
            conn.execute(&sql, &[])
                .map(|_| ())
                .map_err(|e| DbError::QueryError(format!("{} failed: {}", sql, e)))
        })
        .await
    }

    async fn rollback_to_savepoint(&self, name: &str) -> Result<(), DbError> {
        validate_savepoint_name(name)?;
        let sql = format!("ROLLBACK TO SAVEPOINT {}", name);
        self.with_conn(move |conn| {
            conn.execute(&sql, &[])
                .map(|_| ())
                .map_err(|e| DbError::QueryError(format!("{} failed: {}", sql, e)))
        })
        .await
    }

    async fn commit(&self) -> Result<(), DbError> {
        self.with_conn(|conn| conn.commit().map_err(|e| DbError::QueryError(format!("COMMIT failed: {}", e))))
            .await
    }

    async fn rollback(&self) -> Result<(), DbError> {
        self.with_conn(|conn| conn.rollback().map_err(|e| DbError::QueryError(format!("ROLLBACK failed: {}", e))))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_json_preserves_precision() {
        assert_eq!(number_to_json("42"), serde_json::json!(42));
        assert_eq!(number_to_json("49.99"), serde_json::json!(49.99));
        assert_eq!(number_to_json(".5"), serde_json::json!(0.5));
        // 38-digit NUMBER does not fit in i64/f64 exactly: kept as text.
        let big = "12345678901234567890123456789012345678";
        assert_eq!(number_to_json(big), serde_json::json!(big));
        assert_eq!(number_to_json("0.1000000000000000055511151231257827"), serde_json::json!("0.1000000000000000055511151231257827"));
    }

    #[test]
    fn datetimes_are_tidied() {
        assert_eq!(tidy_datetime("2016-05-16T00:00:00.000000000"), "2016-05-16");
        assert_eq!(tidy_datetime("2016-05-16T13:45:09.000000000"), "2016-05-16 13:45:09");
        assert_eq!(tidy_datetime("2016-05-16T13:45:09.120000000"), "2016-05-16 13:45:09.12");
    }

    #[test]
    fn dotted_bind_names_are_rejected() {
        let stmt = BoundSql::with_binds("SELECT 1 FROM DUAL WHERE :BLOCK.ITEM = 1", vec![(":BLOCK.ITEM".into(), SqlValue::Null)]);
        assert!(prepare_binds(&stmt).is_err());
        let ok = BoundSql::with_binds("SELECT 1 FROM DUAL WHERE :b1 = 1", vec![(":b1".into(), SqlValue::Null)]);
        assert!(prepare_binds(&ok).is_ok());
    }
}
