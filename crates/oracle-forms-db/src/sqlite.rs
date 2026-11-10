use crate::driver::DatabaseDriver;
use crate::model::{BoundSql, DbBackend, DbError, QueryRecord, QueryResult, SqlValue};
use rusqlite::{types::ValueRef, Connection, ToSql};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct SqliteDriver {
    conn: Arc<Mutex<Connection>>,
    #[allow(dead_code)]
    path: Option<PathBuf>,
}

impl SqliteDriver {
    pub fn in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;
        register_oracle_functions(&conn)?;
        let driver = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: None,
        };
        driver.seed_default_schema()?;
        Ok(driver)
    }

    pub fn from_file(path: impl Into<PathBuf>) -> Result<Self, DbError> {
        let p = path.into();
        let conn = Connection::open(&p)
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;
        register_oracle_functions(&conn)?;
        let driver = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: Some(p),
        };
        Ok(driver)
    }

    /// Seeds standard schema tables matching Oracle test fixtures (test_items, orders).
    pub fn seed_default_schema(&self) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS test_items (
                item_id INTEGER PRIMARY KEY,
                item_name TEXT NOT NULL,
                amount REAL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );

            INSERT OR IGNORE INTO test_items (item_id, item_name, amount) VALUES
                (1, 'Widget A', 10.50),
                (2, 'Gadget B', 99.50),
                (3, 'Gizmo C', 25.00),
                (4, 'Doohickey D', 4.99),
                (5, 'Thingamajig E', 150.00);

            CREATE TABLE IF NOT EXISTS orders (
                order_id INTEGER PRIMARY KEY,
                customer_id INTEGER DEFAULT 101,
                customer_name TEXT NOT NULL,
                order_date TEXT,
                status TEXT DEFAULT 'ACTIVE',
                order_status TEXT DEFAULT 'ACTIVE',
                total_amount REAL,
                order_total REAL
            );

            INSERT OR IGNORE INTO orders (order_id, customer_id, customer_name, order_date, status, order_status, total_amount, order_total) VALUES
                (1001, 101, 'Acme Industrial', '2026-01-15', 'ACTIVE', 'COMPLETED', 1250.00, 1250.00),
                (1002, 102, 'Globex Corporation', '2026-02-01', 'ACTIVE', 'PENDING', 4300.50, 4300.50),
                (1003, 103, 'Soylent Corp', '2026-02-18', 'CLOSED', 'CANCELLED', 890.00, 890.00),
                (1004, 104, 'Initech LLC', '2026-03-05', 'ACTIVE', 'PROCESSING', 620.25, 620.25);

            CREATE TABLE IF NOT EXISTS departments (
                department_id INTEGER PRIMARY KEY,
                department_name TEXT NOT NULL,
                location_id INTEGER
            );

            INSERT OR IGNORE INTO departments (department_id, department_name, location_id) VALUES
                (10, 'Administration', 1700),
                (20, 'Marketing', 1800),
                (30, 'Purchasing', 1700),
                (60, 'Information Technology', 1400);

            CREATE TABLE IF NOT EXISTS employees (
                employee_id INTEGER PRIMARY KEY,
                first_name TEXT,
                last_name TEXT NOT NULL,
                salary REAL,
                department_id INTEGER
            );

            INSERT OR IGNORE INTO employees (employee_id, first_name, last_name, salary, department_id) VALUES
                (100, 'Steven', 'King', 24000.00, 10),
                (101, 'Neena', 'Kochhar', 17000.00, 10),
                (102, 'Lex', 'De Haan', 17000.00, 10),
                (201, 'Michael', 'Hartstein', 13000.00, 20),
                (202, 'Pat', 'Fay', 6000.00, 20),
                (114, 'Den', 'Raphaely', 11000.00, 30),
                (103, 'Alexander', 'Hunold', 9000.00, 60),
                (104, 'Bruce', 'Ernst', 6000.00, 60);

            CREATE TABLE IF NOT EXISTS items (
                item_id INTEGER PRIMARY KEY,
                item_name TEXT NOT NULL,
                amount REAL
            );

            INSERT OR IGNORE INTO items (item_id, item_name, amount) VALUES
                (101, 'Widget A', 50.00),
                (102, 'Widget B', 75.00),
                (555, 'Original', 10.00);
            "#,
        )
        .map_err(|e| DbError::QueryError(format!("Schema seeding failed: {}", e)))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl DatabaseDriver for SqliteDriver {
    async fn test_connection(&self) -> Result<(), DbError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let locked = conn.lock().unwrap();
            locked
                .query_row("SELECT 1", [], |_| Ok(()))
                .map_err(|e| DbError::ConnectionError(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
    }

    async fn query(&self, stmt: &BoundSql) -> Result<QueryResult, DbError> {
        let sql_owned = stmt.sql.clone();
        let binds_owned = stmt.binds.clone();
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let locked = conn.lock().unwrap();
            let mut statement = locked
                .prepare(&sql_owned)
                .map_err(|e| DbError::QueryError(format!("Prepare error: {}", e)))?;

            let column_names: Vec<String> = statement
                .column_names()
                .iter()
                .map(|s| s.to_string())
                .collect();

            let rusqlite_params = to_sqlite_params(&binds_owned);
            let param_refs: Vec<&dyn ToSql> = rusqlite_params.iter().map(|b| b.as_ref()).collect();

            let is_select = sql_owned
                .trim_start()
                .to_ascii_uppercase()
                .starts_with("SELECT");

            if !is_select {
                statement
                    .execute(rusqlite::params_from_iter(param_refs))
                    .map_err(|e| DbError::QueryError(format!("DML execution error: {}", e)))?;

                return Ok(QueryResult {
                    block_name: String::new(),
                    table_name: String::new(),
                    columns: vec![],
                    records: vec![],
                    total_count: 0,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    sql_statement: sql_owned,
                });
            }

            let mut rows = statement
                .query(rusqlite::params_from_iter(param_refs))
                .map_err(|e| DbError::QueryError(format!("Query execution error: {}", e)))?;

            let mut records = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| DbError::QueryError(format!("Row fetch error: {}", e)))?
            {
                let mut values = HashMap::new();
                let mut row_id_val = None;

                for (idx, col_name) in column_names.iter().enumerate() {
                    let val_ref = row.get_ref(idx).map_err(|e| {
                        DbError::QueryError(format!("Column read error: {}", e))
                    })?;

                    let json_val = match val_ref {
                        ValueRef::Null => serde_json::Value::Null,
                        ValueRef::Integer(i) => serde_json::Value::from(i),
                        ValueRef::Real(r) => serde_json::Value::from(r),
                        ValueRef::Text(t) => {
                            let s = String::from_utf8_lossy(t).to_string();
                            serde_json::Value::from(s)
                        }
                        ValueRef::Blob(b) => {
                            serde_json::Value::from(format!("<BLOB {} bytes>", b.len()))
                        }
                    };

                    let col_upper = col_name.to_ascii_uppercase();
                    if col_upper == "ROWID" {
                        row_id_val = Some(json_val.to_string().trim_matches('"').to_string());
                    }
                    values.insert(col_upper, json_val);
                }

                records.push(QueryRecord {
                    values,
                    row_id: row_id_val,
                });
            }

            let elapsed = start.elapsed().as_millis() as u64;
            let total = records.len();

            Ok(QueryResult {
                block_name: String::new(),
                table_name: String::new(),
                columns: column_names,
                records,
                total_count: total,
                execution_time_ms: elapsed,
                sql_statement: sql_owned,
            })
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
    }

    fn backend(&self) -> DbBackend {
        DbBackend::Sqlite
    }

    fn supports_transactions(&self) -> bool {
        true
    }

    async fn execute(&self, stmt: &BoundSql) -> Result<u64, DbError> {
        let sql_owned = stmt.sql.clone();
        let binds_owned = stmt.binds.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let locked = conn.lock().unwrap();
            let params = to_sqlite_params(&binds_owned);
            let refs: Vec<&dyn ToSql> = params.iter().map(|b| b.as_ref()).collect();
            let affected = locked
                .execute(&sql_owned, rusqlite::params_from_iter(refs))
                .map_err(|e| DbError::QueryError(format!("DML execution error: {}", e)))?;
            Ok(affected as u64)
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
    }

    async fn savepoint(&self, name: &str) -> Result<(), DbError> {
        crate::driver::validate_savepoint_name(name)?;
        self.exec_batch(format!("SAVEPOINT {}", name)).await
    }

    async fn rollback_to_savepoint(&self, name: &str) -> Result<(), DbError> {
        crate::driver::validate_savepoint_name(name)?;
        self.exec_batch(format!("ROLLBACK TO SAVEPOINT {}", name)).await
    }

    async fn commit(&self) -> Result<(), DbError> {
        self.end_transaction("COMMIT").await
    }

    async fn rollback(&self) -> Result<(), DbError> {
        self.end_transaction("ROLLBACK").await
    }
}

impl SqliteDriver {
    async fn exec_batch(&self, sql: String) -> Result<(), DbError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            conn.lock()
                .unwrap()
                .execute_batch(&sql)
                .map_err(|e| DbError::QueryError(format!("{}: {}", sql, e)))
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
    }

    /// COMMIT/ROLLBACK are only valid inside a transaction in SQLite; outside one they
    /// are a no-op, matching Oracle (where COMMIT with nothing pending succeeds).
    async fn end_transaction(&self, verb: &'static str) -> Result<(), DbError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let locked = conn.lock().unwrap();
            if locked.is_autocommit() {
                return Ok(());
            }
            locked
                .execute_batch(verb)
                .map_err(|e| DbError::QueryError(format!("{}: {}", verb, e)))
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
    }
}

/// Formats a number like Oracle's implicit TO_CHAR: no trailing zeros, no leading zero
/// before the decimal point (`25.0` -> `25`, `0.5` -> `.5`).
fn oracle_number_text(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        return format!("{}", f as i64);
    }
    let mut s = format!("{}", f);
    if let Some(rest) = s.strip_prefix("0.") {
        s = format!(".{}", rest);
    } else if let Some(rest) = s.strip_prefix("-0.") {
        s = format!("-.{}", rest);
    }
    s
}

/// Registers simple versions of common Oracle SQL functions so forms written for Oracle
/// can be previewed offline against SQLite. Format models are not supported: a
/// `TO_CHAR(x, 'fmt')` call fails with a clear error instead of returning wrong text.
fn register_oracle_functions(conn: &Connection) -> Result<(), DbError> {
    use rusqlite::functions::FunctionFlags;
    use rusqlite::types::Value;
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    let err = |e: rusqlite::Error| DbError::ConnectionError(format!("Failed to register SQLite function: {}", e));

    conn.create_scalar_function("TO_CHAR", -1, flags, |ctx| {
        if ctx.len() > 1 {
            return Err(rusqlite::Error::UserFunctionError(
                "TO_CHAR with a format model is not supported by the SQLite preview backend".into(),
            ));
        }
        Ok(match ctx.get::<Value>(0)? {
            Value::Null => Value::Null,
            Value::Integer(i) => Value::Text(i.to_string()),
            Value::Real(f) => Value::Text(oracle_number_text(f)),
            Value::Text(t) => Value::Text(t),
            Value::Blob(_) => return Err(rusqlite::Error::UserFunctionError("TO_CHAR of a BLOB".into())),
        })
    })
    .map_err(err)?;
    conn.create_scalar_function("TO_NUMBER", 1, flags, |ctx| {
        Ok(match ctx.get::<Value>(0)? {
            Value::Text(t) => {
                let t = t.trim();
                if let Ok(i) = t.parse::<i64>() {
                    Value::Integer(i)
                } else if let Ok(f) = t.parse::<f64>() {
                    Value::Real(f)
                } else {
                    return Err(rusqlite::Error::UserFunctionError(format!("ORA-01722: invalid number '{}'", t).into()));
                }
            }
            other => other,
        })
    })
    .map_err(err)?;
    conn.create_scalar_function("NVL", 2, flags, |ctx| {
        let a = ctx.get::<Value>(0)?;
        Ok(if matches!(a, Value::Null) { ctx.get::<Value>(1)? } else { a })
    })
    .map_err(err)?;
    conn.create_scalar_function("NVL2", 3, flags, |ctx| {
        let a = ctx.get::<Value>(0)?;
        Ok(if matches!(a, Value::Null) { ctx.get::<Value>(2)? } else { ctx.get::<Value>(1)? })
    })
    .map_err(err)?;
    Ok(())
}

/// Converts bound values into rusqlite parameters (positional, in bind order).
fn to_sqlite_params(binds: &[(String, SqlValue)]) -> Vec<Box<dyn ToSql>> {
    binds
        .iter()
        .map(|(_name, val)| -> Box<dyn ToSql> {
            match val {
                SqlValue::Null => Box::new(rusqlite::types::Null),
                SqlValue::Text(t) => Box::new(t.clone()),
                SqlValue::Number(n) => {
                    if let Ok(i) = n.parse::<i64>() {
                        Box::new(i)
                    } else if let Ok(f) = n.parse::<f64>() {
                        Box::new(f)
                    } else {
                        Box::new(n.clone())
                    }
                }
                SqlValue::Date(d) => Box::new(d.clone()),
                SqlValue::Boolean(b) => Box::new(*b),
            }
        })
        .collect()
}
