use crate::driver::DatabaseDriver;
use crate::generator::SqlGenerator;
use crate::model::{DbBackend, DbConfig, DbError, QueryRecord, QueryResult};
use oracle_forms_core::model::BlockInfo;
use std::collections::HashMap;
use std::process::Command;
use std::time::Instant;
use tokio::net::TcpStream;

pub struct OracleDriver {
    config: DbConfig,
}

impl OracleDriver {
    pub fn new(config: DbConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl DatabaseDriver for OracleDriver {
    async fn test_connection(&self) -> Result<(), DbError> {
        let host = self.config.host.as_deref().unwrap_or("localhost");
        let port = self.config.port.unwrap_or(1521);
        let addr = format!("{}:{}", host, port);

        match TcpStream::connect(&addr).await {
            Ok(_) => Ok(()),
            Err(e) => Err(DbError::ConnectionError(format!(
                "Cannot connect to Oracle listener at {}: {}",
                addr, e
            ))),
        }
    }

    async fn query(&self, sql: &str) -> Result<QueryResult, DbError> {
        let start = Instant::now();
        let user = self.config.username.as_deref().unwrap_or("forms_user");
        let pass = self.config.password.as_deref().unwrap_or("Welcome123#");
        let service = self.config.service_name.as_deref().unwrap_or("FREEPDB1");

        // Format a JSON query script for Oracle Database 23ai
        let sql_trimmed = sql.trim().trim_end_matches(';');
        let script = format!(
            "SET PAGESIZE 0 FEEDBACK OFF VERIFY OFF HEADING OFF ECHO OFF TRIMSPOOL ON LINESIZE 32767 LONG 2000000;\n\
             SELECT json_object(*) FROM ({}) Q_WRAP;\n\
             EXIT;\n",
            sql_trimmed
        );

        let user_auth = format!("{}/{}@{}", user, pass, service);

        let output = tokio::task::spawn_blocking(move || {
            // First attempt: direct podman exec against the running oracle23ai container
            let child = Command::new("podman")
                .args(["exec", "-i", "oracle23ai-lite", "sqlplus", "-s", &user_auth])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            match child {
                Ok(mut proc) => {
                    use std::io::Write;
                    if let Some(mut stdin) = proc.stdin.take() {
                        let _ = stdin.write_all(script.as_bytes());
                    }
                    proc.wait_with_output()
                }
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
        .map_err(|e| DbError::ConnectionError(format!("Failed to execute sqlplus query: {}", e)))?;

        if !output.status.success() {
            let err_str = String::from_utf8_lossy(&output.stderr);
            return Err(DbError::QueryError(format!(
                "Oracle query failed: {}",
                err_str
            )));
        }

        let out_str = String::from_utf8_lossy(&output.stdout);
        let mut records = Vec::new();
        let mut columns = Vec::new();

        for line in out_str.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("ERROR:") || trimmed.starts_with("ORA-") {
                if trimmed.starts_with("ORA-") {
                    return Err(DbError::QueryError(trimmed.to_string()));
                }
                continue;
            }

            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(obj) = json_val.as_object() {
                    let mut values = HashMap::new();
                    for (k, v) in obj {
                        let k_upper = k.to_ascii_uppercase();
                        if !columns.contains(&k_upper) {
                            columns.push(k_upper.clone());
                        }
                        values.insert(k_upper, v.clone());
                    }
                    records.push(QueryRecord {
                        values,
                        row_id: None,
                    });
                }
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;
        let total = records.len();

        Ok(QueryResult {
            block_name: String::new(),
            table_name: String::new(),
            columns,
            records,
            total_count: total,
            execution_time_ms: elapsed,
            sql_statement: sql.to_string(),
        })
    }

    async fn query_block(
        &self,
        block: &BlockInfo,
        limit: Option<usize>,
    ) -> Result<QueryResult, DbError> {
        let generated = SqlGenerator::generate_select(block, DbBackend::Oracle, limit)?;
        let mut result = self.query(&generated.sql).await?;
        result.block_name = block.name.clone();
        result.table_name = generated.table_name;

        // Map column values to corresponding item names if distinct
        for record in &mut result.records {
            for (col, item) in &generated.column_to_item {
                if col != item {
                    if let Some(val) = record.values.get(col).cloned() {
                        record.values.insert(item.clone(), val);
                    }
                }
            }
        }

        Ok(result)
    }

    fn backend(&self) -> DbBackend {
        DbBackend::Oracle
    }
}
