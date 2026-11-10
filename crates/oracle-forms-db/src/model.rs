use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlValue {
    Null,
    Text(String),
    Number(String), // Preserves full decimal precision
    Date(String),   // ISO-8601 representation
    Boolean(bool),
}

impl From<&str> for SqlValue {
    fn from(s: &str) -> Self {
        SqlValue::Text(s.to_string())
    }
}

impl From<String> for SqlValue {
    fn from(s: String) -> Self {
        SqlValue::Text(s)
    }
}

impl From<i64> for SqlValue {
    fn from(n: i64) -> Self {
        SqlValue::Number(n.to_string())
    }
}

impl From<f64> for SqlValue {
    fn from(n: f64) -> Self {
        SqlValue::Number(n.to_string())
    }
}

impl From<bool> for SqlValue {
    fn from(b: bool) -> Self {
        SqlValue::Boolean(b)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundSql {
    pub sql: String,
    pub binds: Vec<(String, SqlValue)>,
}

impl BoundSql {
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            binds: Vec::new(),
        }
    }

    pub fn with_binds(sql: impl Into<String>, binds: Vec<(String, SqlValue)>) -> Self {
        Self {
            sql: sql.into(),
            binds,
        }
    }
}

impl From<&str> for BoundSql {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for BoundSql {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DbBackend {
    #[default]
    Mock,
    Sqlite,
    Oracle,
    #[serde(alias = "oracle-thin")]
    OracleThin,
    Postgres,
}

impl DbBackend {
    /// Parses a user-supplied backend name. Unknown names are an error rather than a
    /// silent fallback to the mock backend, so callers never show fake rows by accident.
    pub fn parse(name: &str) -> Result<Self, DbError> {
        match name.trim().to_ascii_lowercase().as_str() {
            "mock" => Ok(DbBackend::Mock),
            "sqlite" => Ok(DbBackend::Sqlite),
            "oracle" | "testbed" | "testbed-sqlplus" => Ok(DbBackend::Oracle),
            "oracle-thin" | "oracle_thin" | "thin" => Ok(DbBackend::OracleThin),
            "postgres" | "postgresql" => Ok(DbBackend::Postgres),
            other => Err(DbError::ConfigError(format!(
                "Unknown database backend '{}'. Expected one of: mock, sqlite, oracle, oracle-thin",
                other
            ))),
        }
    }
}

/// Connection settings for a database backend.
///
/// Credentials are never defaulted: `username` and `password` are `None` unless the caller
/// supplies them or explicitly opts in to environment credentials via
/// [`DbConfig::with_env_credentials`].
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbConfig {
    #[serde(default)]
    pub backend: DbBackend,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub service_name: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database_path: Option<String>,
}

impl std::fmt::Debug for DbConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConfig")
            .field("backend", &self.backend)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("service_name", &self.service_name)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("database_path", &self.database_path)
            .finish()
    }
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            backend: DbBackend::Mock,
            host: Some("localhost".to_string()),
            port: Some(1521),
            service_name: Some("FREEPDB1".to_string()),
            username: None,
            password: None,
            database_path: None,
        }
    }
}

impl DbConfig {
    pub fn oracle_default() -> Self {
        Self {
            backend: DbBackend::Oracle,
            ..Default::default()
        }
    }

    pub fn oracle_thin_default() -> Self {
        Self {
            backend: DbBackend::OracleThin,
            ..Default::default()
        }
    }

    pub fn sqlite_memory() -> Self {
        Self {
            backend: DbBackend::Sqlite,
            host: None,
            port: None,
            service_name: None,
            username: None,
            password: None,
            database_path: Some(":memory:".to_string()),
        }
    }

    pub fn mock() -> Self {
        Self {
            backend: DbBackend::Mock,
            ..Default::default()
        }
    }

    /// Fills any missing username/password from `OFW_DB_USER` / `OFW_DB_PASSWORD`.
    ///
    /// Intended for local, user-invoked tools (the CLI, opt-in server mode, live tests).
    /// Values supplied explicitly are never overridden.
    pub fn with_env_credentials(mut self) -> Self {
        if self.username.is_none() {
            self.username = std::env::var("OFW_DB_USER").ok().filter(|v| !v.is_empty());
        }
        if self.password.is_none() {
            self.password = std::env::var("OFW_DB_PASSWORD").ok().filter(|v| !v.is_empty());
        }
        self
    }

    /// Returns `(username, password)` or a configuration error naming what is missing.
    pub fn require_credentials(&self) -> Result<(String, String), DbError> {
        let user = self
            .username
            .clone()
            .filter(|u| !u.trim().is_empty())
            .ok_or_else(|| {
                DbError::ConfigError(
                    "Database username not provided. Supply it in the request or set OFW_DB_USER (CLI)."
                        .to_string(),
                )
            })?;
        let pass = self.password.clone().filter(|p| !p.is_empty()).ok_or_else(|| {
            DbError::ConfigError(
                "Database password not provided. Supply it in the request or set OFW_DB_PASSWORD (CLI)."
                    .to_string(),
            )
        })?;
        Ok((user, pass))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryRecord {
    pub values: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub block_name: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub records: Vec<QueryRecord>,
    pub total_count: usize,
    pub execution_time_ms: u64,
    pub sql_statement: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database connection failed: {0}")]
    ConnectionError(String),

    #[error("SQL query execution failed: {0}")]
    QueryError(String),

    #[error("Block has no valid query data source")]
    MissingDataSource,

    #[error("No database columns found for block '{0}'")]
    NoDatabaseColumns(String),

    #[error("Unsupported database backend: {0:?}")]
    UnsupportedBackend(DbBackend),

    #[error("Internal database error: {0}")]
    Internal(String),

    #[error("Database configuration error: {0}")]
    ConfigError(String),
}
