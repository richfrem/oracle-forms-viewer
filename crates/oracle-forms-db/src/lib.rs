pub mod driver;
pub mod generator;
pub mod mock;
pub mod model;
pub mod oracle;
pub mod sqlite;

pub use driver::DatabaseDriver;
pub use generator::{GeneratedQuery, SqlGenerator};
pub use mock::MockDriver;
pub use model::{DbBackend, DbConfig, DbError, QueryRecord, QueryResult};
pub use oracle::OracleDriver;
pub use sqlite::SqliteDriver;

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
        DbBackend::Oracle => Ok(Box::new(OracleDriver::new(config.clone()))),
        DbBackend::Postgres => Err(DbError::UnsupportedBackend(DbBackend::Postgres)),
    }
}
