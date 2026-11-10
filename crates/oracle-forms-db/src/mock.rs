use crate::driver::DatabaseDriver;
use crate::model::{BoundSql, DbBackend, DbError, QueryRecord, QueryResult};
use crate::sqlite::SqliteDriver;
use oracle_forms_core::model::BlockInfo;
use std::collections::HashMap;

pub struct MockDriver {
    sqlite: SqliteDriver,
}

impl MockDriver {
    pub fn new() -> Result<Self, DbError> {
        let sqlite = SqliteDriver::in_memory()?;
        Ok(Self { sqlite })
    }
}

#[async_trait::async_trait]
impl DatabaseDriver for MockDriver {
    async fn test_connection(&self) -> Result<(), DbError> {
        self.sqlite.test_connection().await
    }

    async fn query(&self, stmt: &BoundSql) -> Result<QueryResult, DbError> {
        self.sqlite.query(stmt).await
    }

    async fn query_block(
        &self,
        block: &BlockInfo,
        limit: Option<usize>,
    ) -> Result<QueryResult, DbError> {
        // Query the seeded SQLite tables; if the block's table is not one of them,
        // synthesize clearly-labelled sample rows (this is the Mock backend).
        match self.sqlite.query_block(block, limit).await {
            Ok(res) if !res.records.is_empty() => Ok(res),
            _ => {
                // If table doesn't exist, synthesize plausible mock records based on block items
                let mut records = Vec::new();
                let count = limit.unwrap_or(5).min(10);

                for i in 1..=count {
                    let mut values = HashMap::new();
                    for item in &block.items {
                        let is_btn = item
                            .item_type
                            .as_deref()
                            .map(|s| s.eq_ignore_ascii_case("Push Button"))
                            .unwrap_or(false);
                        if is_btn {
                            continue;
                        }

                        let val = match item.data_type.as_deref().unwrap_or("Char") {
                            "Number" => serde_json::Value::from(1000 + i),
                            "Date" => serde_json::Value::from(format!("2026-03-{:02}", i)),
                            _ => serde_json::Value::from(format!("Sample {} #{}", item.name, i)),
                        };

                        values.insert(item.name.clone(), val.clone());
                        if let Some(col) = &item.column_name {
                            values.insert(col.to_ascii_uppercase(), val);
                        }
                    }
                    records.push(QueryRecord {
                        values,
                        row_id: Some(format!("ROW_{:04}", i)),
                    });
                }

                let columns: Vec<String> = block.items.iter().map(|i| i.name.clone()).collect();

                Ok(QueryResult {
                    block_name: block.name.clone(),
                    table_name: block.query_data_source_name.clone().unwrap_or_else(|| block.name.clone()),
                    columns,
                    records,
                    total_count: count,
                    execution_time_ms: 2,
                    sql_statement: format!("MOCK_SYNTHESIZED_SELECT FROM {}", block.name),
                })
            }
        }
    }

    fn backend(&self) -> DbBackend {
        DbBackend::Mock
    }

    // The mock is backed by an in-memory SQLite database, so it supports real
    // transactions too; DML against the seeded tables behaves like a real database.
    fn supports_transactions(&self) -> bool {
        true
    }

    async fn execute(&self, stmt: &BoundSql) -> Result<u64, DbError> {
        self.sqlite.execute(stmt).await
    }

    async fn savepoint(&self, name: &str) -> Result<(), DbError> {
        self.sqlite.savepoint(name).await
    }

    async fn rollback_to_savepoint(&self, name: &str) -> Result<(), DbError> {
        self.sqlite.rollback_to_savepoint(name).await
    }

    async fn commit(&self) -> Result<(), DbError> {
        self.sqlite.commit().await
    }

    async fn rollback(&self) -> Result<(), DbError> {
        self.sqlite.rollback().await
    }
}
