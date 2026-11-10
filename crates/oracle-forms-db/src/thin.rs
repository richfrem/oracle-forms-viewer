use crate::driver::DatabaseDriver;
use crate::model::{BoundSql, DbBackend, DbConfig, DbError, QueryRecord, QueryResult, SqlValue};
use oracledb::{OracleTimestamp, Pool, PoolConfig, ToDbValue};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Pure-Rust Oracle Thin Driver implementing zero-client native wire protocol over TCP :1521.
///
/// Communicates directly with Oracle Database Net listeners (19c/21c/23ai) using the
/// official pure-Rust `oracledb` crate without JVM, C libraries, Instant Client, or SQL*Plus.
pub struct OracleThinDriver {
    config: DbConfig,
    pool: Arc<Mutex<Option<Pool>>>,
}

impl OracleThinDriver {
    pub fn new(config: DbConfig) -> Self {
        Self {
            config,
            pool: Arc::new(Mutex::new(None)),
        }
    }

    /// Resolves connection parameters from `DbConfig` and environment variables.
    fn resolve_parameters(&self) -> (String, String, String, u16, String) {
        let user = self
            .config
            .username
            .clone()
            .or_else(|| std::env::var("OFW_DB_USER").ok())
            .unwrap_or_else(|| "forms_user".to_string());

        let pass = self
            .config
            .password
            .clone()
            .or_else(|| std::env::var("OFW_DB_PASSWORD").ok())
            .unwrap_or_else(|| "Welcome123#".to_string());

        let host = self
            .config
            .host
            .clone()
            .unwrap_or_else(|| "127.0.0.1".to_string());

        let port = self.config.port.unwrap_or(1521);

        let service = self
            .config
            .service_name
            .clone()
            .unwrap_or_else(|| "FREEPDB1".to_string());

        (user, pass, host, port, service)
    }
}

fn acquire_pooled_conn(
    pool: &Arc<Mutex<Option<Pool>>>,
    params: &(String, String, String, u16, String),
) -> Result<oracledb::Connection, DbError> {
    let mut guard = pool
        .lock()
        .map_err(|e| DbError::Internal(format!("Failed to lock Oracle pool mutex: {}", e)))?;

    if guard.is_none() {
        let (user, pass, host, port, service) = params;
        let connect_string = format!("{}:{}/{}", host, port, service);

        let pool_cfg = PoolConfig::default()
            .set_credentials(user, pass)
            .set_connect_string(&connect_string)
            .map_err(|e| {
                DbError::ConnectionError(format!(
                    "Invalid Oracle thin connect string '{}': {}",
                    connect_string, e
                ))
            })?
            .set_min_connections(1)
            .set_max_connections(10)
            .set_ping_interval(Some(Duration::from_secs(30)));

        let new_pool = oracledb::create_pool(pool_cfg).map_err(|e| {
            DbError::ConnectionError(format!(
                "Failed to initialize Oracle thin connection pool to '{}': {}",
                connect_string, e
            ))
        })?;
        *guard = Some(new_pool);
    }

    guard
        .as_ref()
        .unwrap()
        .acquire()
        .map_err(|e| DbError::ConnectionError(format!("Failed to acquire Oracle connection from pool: {}", e)))
}

#[async_trait::async_trait]
impl DatabaseDriver for OracleThinDriver {
    async fn test_connection(&self) -> Result<(), DbError> {
        let pool = Arc::clone(&self.pool);
        let params = self.resolve_parameters();

        tokio::task::spawn_blocking(move || {
            let conn = acquire_pooled_conn(&pool, &params)?;

            let row = conn
                .query_row("SELECT 1 FROM DUAL", &[])
                .map_err(|e| DbError::QueryError(format!("Oracle ping query failed: {}", e)))?;
            let val: i32 = row
                .get(0)
                .map_err(|e| DbError::QueryError(format!("Failed to read ping result: {}", e)))?;
            if val != 1 {
                return Err(DbError::ConnectionError(format!(
                    "Unexpected ping response from Oracle: {}",
                    val
                )));
            }

            Ok(())
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
    }

    async fn query(&self, stmt: &BoundSql) -> Result<QueryResult, DbError> {
        let pool = Arc::clone(&self.pool);
        let params = self.resolve_parameters();
        let stmt_clone = stmt.clone();

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let conn = acquire_pooled_conn(&pool, &params)?;

            // Prepare binds
            let bind_data: Vec<(String, Option<String>)> = stmt_clone
                .binds
                .iter()
                .map(|(name, val)| {
                    let clean = name.strip_prefix(':').unwrap_or(name).to_string();
                    let val_str = match val {
                        SqlValue::Null => None,
                        SqlValue::Text(s) => Some(s.clone()),
                        SqlValue::Number(n) => Some(n.clone()),
                        SqlValue::Date(d) => Some(d.clone()),
                        SqlValue::Boolean(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
                    };
                    (clean, val_str)
                })
                .collect();

            let named_binds: Vec<(&str, &dyn ToDbValue)> = bind_data
                .iter()
                .map(|(k, v)| {
                    let db_val: &dyn ToDbValue = v;
                    (k.as_str(), db_val)
                })
                .collect();

            let cursor = if named_binds.is_empty() {
                conn.query(&stmt_clone.sql, &[])
            } else {
                conn.query_named(&stmt_clone.sql, &named_binds)
            }
            .map_err(|e| DbError::QueryError(format!("Oracle query execution failed: {}", e)))?;

            let columns: Vec<String> = cursor
                .columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect();

            let mut records = Vec::new();

            for row_result in cursor {
                let row = row_result
                    .map_err(|e| DbError::QueryError(format!("Failed to fetch Oracle row: {}", e)))?;

                let mut values = HashMap::new();
                let mut row_id_val = None;

                for (idx, col_name) in columns.iter().enumerate() {
                    let str_val: Option<String> = row.get(idx).ok().flatten();
                    let final_str = if let Some(s) = str_val {
                        Some(s)
                    } else if let Ok(Some(ts)) = row.get::<Option<OracleTimestamp>>(idx) {
                        Some(ts.to_string())
                    } else {
                        None
                    };

                    let k_upper = col_name.to_ascii_uppercase();
                    if k_upper == "ROWID" || k_upper == "OFW_ROWID" {
                        row_id_val = final_str.clone();
                    }

                    let json_val = match final_str {
                        None => serde_json::Value::Null,
                        Some(s) => {
                            if s.eq_ignore_ascii_case("true") {
                                serde_json::Value::Bool(true)
                            } else if s.eq_ignore_ascii_case("false") {
                                serde_json::Value::Bool(false)
                            } else if let Ok(num) = s.parse::<i64>() {
                                serde_json::Value::Number(num.into())
                            } else if let Ok(num) = s.parse::<f64>() {
                                serde_json::Number::from_f64(num)
                                    .map(serde_json::Value::Number)
                                    .unwrap_or(serde_json::Value::String(s))
                            } else {
                                serde_json::Value::String(s)
                            }
                        }
                    };

                    values.insert(k_upper, json_val);
                }

                records.push(QueryRecord {
                    values,
                    row_id: row_id_val,
                });
            }

            let total_count = records.len();
            let execution_time_ms = start.elapsed().as_millis() as u64;

            Ok(QueryResult {
                block_name: String::new(),
                table_name: String::new(),
                columns,
                records,
                total_count,
                execution_time_ms,
                sql_statement: stmt_clone.sql,
            })
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
    }

    fn backend(&self) -> DbBackend {
        DbBackend::OracleThin
    }
}
