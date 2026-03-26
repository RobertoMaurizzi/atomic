//! Storage abstraction layer for atomic-core.
//!
//! This module defines the trait hierarchy for database backends and provides
//! the default SQLite implementation. Alternative backends (e.g., Postgres)
//! can be added by implementing the `Storage` supertrait.

pub mod traits;
pub mod sqlite;
pub mod postgres;

pub use traits::*;
pub use sqlite::SqliteStorage;

#[cfg(feature = "postgres")]
pub use postgres::PostgresStorage;
