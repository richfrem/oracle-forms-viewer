use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbBackend {
    Mock,
    Sqlite,
    Oracle,
    Postgres,
}

impl Default for DbBackend {
    fn default() -> Self {
        DbBackend::Mock
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            backend: DbBackend::Mock,
            host: Some("localhost".to_string()),
            port: Some(1521),
            service_name: Some("FREEPDB1".to_string()),
            username: Some("forms_user".to_string()),
            password: Some("Welcome123#".to_string()),
            database_path: None,
        }
    }
}

impl DbConfig {
    pub fn oracle_default() -> Self {
        Self {
            backend: DbBackend::Oracle,
            host: Some("localhost".to_string()),
            port: Some(1521),
            service_name: Some("FREEPDB1".to_string()),
            username: Some("forms_user".to_string()),
            password: Some("Welcome123#".to_string()),
            database_path: None,
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

    #[error("Unsupported database backend: {0:?}")]
    UnsupportedBackend(DbBackend),

    #[error("Internal database error: {0}")]
    Internal(String),
}
