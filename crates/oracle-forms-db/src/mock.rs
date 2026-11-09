use crate::driver::DatabaseDriver;
use crate::model::{DbBackend, DbError, QueryRecord, QueryResult};
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

    async fn query(&self, sql: &str) -> Result<QueryResult, DbError> {
        self.sqlite.query(sql).await
    }

    async fn query_block(
        &self,
        block: &BlockInfo,
        limit: Option<usize>,
    ) -> Result<QueryResult, DbError> {
        // Special case for our sample test_db (BLOCK5 with TEXT_ITEM4 and TEXT_ITEM284)
        if block.name.eq_ignore_ascii_case("BLOCK5") || block.name.eq_ignore_ascii_case("TEST_BLOCK") {
            let mut result = self.sqlite.query("SELECT item_id, item_name, amount FROM test_items ORDER BY item_id").await?;
            result.block_name = block.name.clone();
            result.table_name = "TEST_ITEMS".to_string();

            // Populate form item names directly:
            // TEXT_ITEM4 receives item_name (e.g. 'Widget A', 'Gadget B')
            // TEXT_ITEM284 receives amount (e.g. 10.5, 99.5)
            for record in &mut result.records {
                if let Some(item_name_val) = record.values.get("ITEM_NAME").cloned() {
                    record.values.insert("TEXT_ITEM4".to_string(), item_name_val);
                }
                if let Some(amount_val) = record.values.get("AMOUNT").cloned() {
                    record.values.insert("TEXT_ITEM284".to_string(), amount_val);
                }
            }

            if let Some(lim) = limit {
                result.records.truncate(lim);
                result.total_count = result.records.len();
            }

            return Ok(result);
        }

        // Standard block resolution via SQLite
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
}
