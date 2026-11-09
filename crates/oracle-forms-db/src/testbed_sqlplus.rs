use crate::driver::DatabaseDriver;
use crate::generator::SqlGenerator;
use crate::model::{BoundSql, DbBackend, DbConfig, DbError, QueryRecord, QueryResult, SqlValue};
use oracle_forms_core::model::BlockInfo;
use std::collections::HashMap;
use std::process::Command;
use std::time::Instant;
use tokio::net::TcpStream;

pub struct TestbedSqlplusDriver {
    config: DbConfig,
}

pub type OracleDriver = TestbedSqlplusDriver;

impl TestbedSqlplusDriver {
    pub fn new(config: DbConfig) -> Self {
        Self { config }
    }

    fn container_name() -> String {
        std::env::var("OFW_TESTBED_DB_CONTAINER").unwrap_or_else(|_| "oracle23ai-lite".to_string())
    }

    fn resolve_credentials(&self) -> Result<(String, String, String), DbError> {
        let user = self
            .config
            .username
            .clone()
            .or_else(|| std::env::var("OFW_DB_USER").ok())
            .unwrap_or_else(|| "forms_user".to_string());

        let pass = match self.config.password.as_deref() {
            Some(p) => p.to_string(),
            None => std::env::var("OFW_DB_PASSWORD").map_err(|_| {
                DbError::ConnectionError(
                    "Database password not provided. Set OFW_DB_PASSWORD or supply password in DbConfig."
                        .to_string(),
                )
            })?,
        };

        let service = self
            .config
            .service_name
            .clone()
            .unwrap_or_else(|| "FREEPDB1".to_string());

        Ok((user, pass, service))
    }
}

#[async_trait::async_trait]
impl DatabaseDriver for TestbedSqlplusDriver {
    async fn test_connection(&self) -> Result<(), DbError> {
        let host = self.config.host.as_deref().unwrap_or("localhost");
        let port = self.config.port.unwrap_or(1521);
        let addr = format!("{}:{}", host, port);

        // 1. Verify TCP reachability of the listener
        TcpStream::connect(&addr).await.map_err(|e| {
            DbError::ConnectionError(format!("Cannot connect to Oracle listener at {}: {}", addr, e))
        })?;

        // 2. Verify actual login credentials via SELECT 1 FROM DUAL using /nolog
        let (user, pass, service) = self.resolve_credentials()?;
        let container = Self::container_name();

        let connect_line = format!("CONNECT {}/{}@{}", user, pass, service);
        let test_script = format!(
            "{}\nSET PAGESIZE 0 FEEDBACK OFF VERIFY OFF HEADING OFF ECHO OFF;\nSELECT 1 FROM DUAL;\nEXIT;\n",
            connect_line
        );

        let output = tokio::task::spawn_blocking(move || {
            let child = Command::new("podman")
                .args(["exec", "-i", &container, "sqlplus", "-s", "/nolog"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            match child {
                Ok(mut proc) => {
                    use std::io::Write;
                    if let Some(mut stdin) = proc.stdin.take() {
                        let _ = stdin.write_all(test_script.as_bytes());
                    }
                    proc.wait_with_output()
                }
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?
        .map_err(|e| DbError::ConnectionError(format!("Failed to execute sqlplus: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if stdout.contains("ERROR:")
            || stdout.contains("ORA-")
            || stderr.contains("ERROR:")
            || stderr.contains("ORA-")
        {
            let err_msg = if !stderr.trim().is_empty() {
                stderr.trim().to_string()
            } else {
                stdout.trim().to_string()
            };
            return Err(DbError::ConnectionError(format!(
                "Oracle login test failed: {}",
                err_msg
            )));
        }

        if !stdout.contains('1') {
            return Err(DbError::ConnectionError(format!(
                "Unexpected response from Oracle login verification: {}",
                stdout.trim()
            )));
        }

        Ok(())
    }

    async fn query(&self, stmt: &BoundSql) -> Result<QueryResult, DbError> {
        let start = Instant::now();
        let (user, pass, service) = self.resolve_credentials()?;
        let container = Self::container_name();

        // Prepare subquery: if query begins with SELECT ROWID, rewrite for subquery json_object wrapping
        let subquery_sql = stmt.sql.replace(['\n', '\r'], " ");
        let subquery_trimmed = subquery_sql.trim().trim_end_matches(';').to_string();

        let rewritten_subquery = if subquery_trimmed.to_ascii_uppercase().starts_with("SELECT ROWID,")
            || subquery_trimmed.to_ascii_uppercase().starts_with("SELECT ROWID ")
        {
            // Replace ROWID with ROWIDTOCHAR(ROWID) AS OFW_ROWID to prevent ORA-01446 in json_object wrapper
            let after_select = &subquery_trimmed[6..]; // skip "SELECT"
            let rowid_stripped = after_select.trim_start();
            let after_rowid = &rowid_stripped[5..]; // skip "ROWID"
            format!("SELECT ROWIDTOCHAR(ROWID) AS OFW_ROWID{}", after_rowid)
        } else {
            subquery_trimmed
        };

        // Format variable declarations and assignments for SQL*Plus
        let mut var_declarations = String::new();
        let mut var_assignments = String::new();

        for (idx, (_name, val)) in stmt.binds.iter().enumerate() {
            let var_name = format!("b{}", idx + 1);
            match val {
                SqlValue::Number(n) => {
                    var_declarations.push_str(&format!("VARIABLE {} NUMBER;\n", var_name));
                    var_assignments.push_str(&format!("EXEC :{} := {};\n", var_name, n));
                }
                SqlValue::Null => {
                    var_declarations.push_str(&format!("VARIABLE {} VARCHAR2(4000);\n", var_name));
                    var_assignments.push_str(&format!("EXEC :{} := NULL;\n", var_name));
                }
                SqlValue::Boolean(b) => {
                    var_declarations.push_str(&format!("VARIABLE {} VARCHAR2(10);\n", var_name));
                    let val_str = if *b { "'Y'" } else { "'N'" };
                    var_assignments.push_str(&format!("EXEC :{} := {};\n", var_name, val_str));
                }
                SqlValue::Text(t) | SqlValue::Date(t) => {
                    var_declarations.push_str(&format!("VARIABLE {} VARCHAR2(4000);\n", var_name));
                    let escaped_t = t.replace('\'', "''");
                    var_assignments.push_str(&format!("EXEC :{} := '{}';\n", var_name, escaped_t));
                }
            }
        }

        let connect_line = format!("CONNECT {}/{}@{}", user, pass, service);
        let script = format!(
            "{}\n\
             SET PAGESIZE 0 FEEDBACK OFF VERIFY OFF HEADING OFF ECHO OFF TRIMSPOOL ON LINESIZE 32767 LONG 2000000;\n\
             {}\
             {}\
             SELECT json_object(*) FROM ({}) Q_WRAP;\n\
             EXIT;\n",
            connect_line, var_declarations, var_assignments, rewritten_subquery
        );

        let output = tokio::task::spawn_blocking(move || {
            let child = Command::new("podman")
                .args(["exec", "-i", &container, "sqlplus", "-s", "/nolog"])
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
                    let mut row_id_val = None;

                    for (k, v) in obj {
                        let k_upper = k.to_ascii_uppercase();
                        if k_upper == "OFW_ROWID" || k_upper == "ROWID" {
                            if let Some(s) = v.as_str() {
                                row_id_val = Some(s.to_string());
                            } else {
                                row_id_val = Some(v.to_string().trim_matches('"').to_string());
                            }
                            if !columns.contains(&"ROWID".to_string()) {
                                columns.push("ROWID".to_string());
                            }
                            values.insert("ROWID".to_string(), v.clone());
                        } else {
                            if !columns.contains(&k_upper) {
                                columns.push(k_upper.clone());
                            }
                            values.insert(k_upper, v.clone());
                        }
                    }

                    records.push(QueryRecord {
                        values,
                        row_id: row_id_val,
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
            sql_statement: stmt.sql.clone(),
        })
    }

    async fn query_block(
        &self,
        block: &BlockInfo,
        limit: Option<usize>,
    ) -> Result<QueryResult, DbError> {
        let generated = SqlGenerator::generate_select(block, DbBackend::Oracle, limit)?;
        let mut result = self.query(&generated.bound_sql).await?;
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
