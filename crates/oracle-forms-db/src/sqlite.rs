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
                customer_name TEXT NOT NULL,
                order_date TEXT,
                status TEXT DEFAULT 'ACTIVE',
                total_amount REAL
            );

            INSERT OR IGNORE INTO orders (order_id, customer_name, order_date, status, total_amount) VALUES
                (1001, 'Acme Industrial', '2026-01-15', 'ACTIVE', 1250.00),
                (1002, 'Globex Corporation', '2026-02-01', 'ACTIVE', 4300.50),
                (1003, 'Soylent Corp', '2026-02-18', 'CLOSED', 890.00),
                (1004, 'Initech LLC', '2026-03-05', 'ACTIVE', 620.25);
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

            // Convert binds into rusqlite ToSql trait objects
            let mut rusqlite_params: Vec<Box<dyn ToSql>> = Vec::new();
            for (_name, val) in &binds_owned {
                match val {
                    SqlValue::Null => rusqlite_params.push(Box::new(rusqlite::types::Null)),
                    SqlValue::Text(t) => rusqlite_params.push(Box::new(t.clone())),
                    SqlValue::Number(n) => {
                        if let Ok(i) = n.parse::<i64>() {
                            rusqlite_params.push(Box::new(i));
                        } else if let Ok(f) = n.parse::<f64>() {
                            rusqlite_params.push(Box::new(f));
                        } else {
                            rusqlite_params.push(Box::new(n.clone()));
                        }
                    }
                    SqlValue::Date(d) => rusqlite_params.push(Box::new(d.clone())),
                    SqlValue::Boolean(b) => rusqlite_params.push(Box::new(*b)),
                }
            }

            let param_refs: Vec<&dyn ToSql> = rusqlite_params.iter().map(|b| b.as_ref()).collect();

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
}
