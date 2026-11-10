pub mod driver;
pub mod generator;
pub mod mock;
pub mod model;
pub mod sqlite;
pub mod testbed_sqlplus;
pub mod thin;

pub use driver::DatabaseDriver;
pub use generator::{GeneratedQuery, SqlGenerator};
pub use mock::MockDriver;
pub use model::{BoundSql, DbBackend, DbConfig, DbError, QueryRecord, QueryResult, SqlValue};
pub use sqlite::SqliteDriver;
pub use testbed_sqlplus::{OracleDriver, TestbedSqlplusDriver};
pub use thin::OracleThinDriver;

/// Factory function to create a DatabaseDriver for the requested configuration.
pub fn create_driver(config: &DbConfig) -> Result<Box<dyn DatabaseDriver>, DbError> {
    match config.backend {
        DbBackend::Mock => Ok(Box::new(MockDriver::new()?)),
        DbBackend::Sqlite => {
            if let Some(path) = &config.database_path {
                if path == ":memory:" {
                    Ok(Box::new(SqliteDriver::in_memory()?))
                } else {
                    Ok(Box::new(SqliteDriver::from_file(path)?))
                }
            } else {
                Ok(Box::new(SqliteDriver::in_memory()?))
            }
        }
        DbBackend::Oracle => Ok(Box::new(TestbedSqlplusDriver::new(config.clone()))),
        DbBackend::OracleThin => Ok(Box::new(OracleThinDriver::new(config.clone()))),
        DbBackend::Postgres => Err(DbError::UnsupportedBackend(DbBackend::Postgres)),
    }
}
