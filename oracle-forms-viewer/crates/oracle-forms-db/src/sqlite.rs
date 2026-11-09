use crate::driver::DatabaseDriver;
use crate::model::{DbBackend, DbError, QueryRecord, QueryResult};
use rusqlite::{types::ValueRef, Connection};
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

    async fn query(&self, sql: &str) -> Result<QueryResult, DbError> {
        let sql_owned = sql.to_string();
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let locked = conn.lock().unwrap();
            let mut stmt = locked
                .prepare(&sql_owned)
                .map_err(|e| DbError::QueryError(format!("Prepare error: {}", e)))?;

            let column_names: Vec<String> = stmt
                .column_names()
                .iter()
                .map(|s| s.to_string())
                .collect();

            let mut rows = stmt
                .query([])
                .map_err(|e| DbError::QueryError(format!("Query execution error: {}", e)))?;

            let mut records = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| DbError::QueryError(format!("Row fetch error: {}", e)))?
            {
                let mut values = HashMap::new();
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

                    values.insert(col_name.to_ascii_uppercase(), json_val);
                }

                records.push(QueryRecord {
                    values,
                    row_id: None,
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
