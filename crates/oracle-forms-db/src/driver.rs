use crate::generator::SqlGenerator;
use crate::model::{BoundSql, DbError, QueryResult};
use oracle_forms_core::model::BlockInfo;

#[async_trait::async_trait]
pub trait DatabaseDriver: Send + Sync {
    /// Tests connectivity with the database backend.
    async fn test_connection(&self) -> Result<(), DbError>;

    /// Executes a SQL query with parameter bindings and returns rows.
    async fn query(&self, stmt: &BoundSql) -> Result<QueryResult, DbError>;

    /// Convenience helper for executing a raw SQL query without binds.
    async fn query_raw(&self, sql: &str) -> Result<QueryResult, DbError> {
        self.query(&BoundSql::new(sql)).await
    }

    /// Generates and executes a query for a specific Form Block.
    async fn query_block(
        &self,
        block: &BlockInfo,
        limit: Option<usize>,
    ) -> Result<QueryResult, DbError> {
        let generated = SqlGenerator::generate_select(block, self.backend(), limit)?;
        if let Some(first) = generated.bind_refs.first() {
            // Without a running form session there is nothing to resolve :BLOCK.ITEM
            // against. Querying with NULL would silently return the wrong rows.
            return Err(DbError::QueryError(format!(
                "Block {} WHERE clause references :{}; run the query from a form session so it can be resolved",
                block.name, first.reference
            )));
        }
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

    /// Returns the database backend type.
    fn backend(&self) -> crate::model::DbBackend;

    // ---------------------------------------------------------------------
    // Transactional API.
    //
    // A driver instance represents ONE database session: every call below runs on the
    // same underlying connection, so locks, DML, savepoints and COMMIT line up the way
    // Oracle Forms expects. Drivers that cannot guarantee this (e.g. the testbed sqlplus
    // driver, which starts a new process per statement) return `DbError::Unsupported`.
    // ---------------------------------------------------------------------

    /// True if this driver supports `execute`, savepoints, commit and rollback on a
    /// single session.
    fn supports_transactions(&self) -> bool {
        false
    }

    /// Executes a DML/DDL statement and returns the number of rows affected.
    async fn execute(&self, _stmt: &BoundSql) -> Result<u64, DbError> {
        Err(DbError::Unsupported(format!(
            "{:?} driver is query-only (no DML or transactions)",
            self.backend()
        )))
    }

    /// Creates a named savepoint in the current transaction.
    async fn savepoint(&self, _name: &str) -> Result<(), DbError> {
        Err(DbError::Unsupported(format!("{:?} driver does not support savepoints", self.backend())))
    }

    /// Rolls back to a named savepoint, keeping the transaction open.
    async fn rollback_to_savepoint(&self, _name: &str) -> Result<(), DbError> {
        Err(DbError::Unsupported(format!("{:?} driver does not support savepoints", self.backend())))
    }

    /// Commits the current transaction.
    async fn commit(&self) -> Result<(), DbError> {
        Err(DbError::Unsupported(format!("{:?} driver does not support commit", self.backend())))
    }

    /// Rolls back the current transaction.
    async fn rollback(&self) -> Result<(), DbError> {
        Err(DbError::Unsupported(format!("{:?} driver does not support rollback", self.backend())))
    }
}

/// Validates a savepoint name so it can be safely embedded in SQL text
/// (savepoint names cannot be bound as parameters).
pub fn validate_savepoint_name(name: &str) -> Result<(), DbError> {
    let mut chars = name.chars();
    let starts_ok = chars.next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false);
    if starts_ok && name.len() <= 30 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(DbError::QueryError(format!("Invalid savepoint name '{}'", name)))
    }
}
