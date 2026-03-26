//! Postgres + pgvector implementation of the Storage traits.
//!
//! This module provides a PostgresStorage backend using sqlx with pgvector
//! for vector similarity search and Postgres built-in tsvector for full-text search.
//! All methods are natively async (no spawn_blocking needed).

#[cfg(feature = "postgres")]
mod atoms;
#[cfg(feature = "postgres")]
mod tags;
#[cfg(feature = "postgres")]
mod chunks;
#[cfg(feature = "postgres")]
mod search;
#[cfg(feature = "postgres")]
mod chat;
#[cfg(feature = "postgres")]
mod wiki;
#[cfg(feature = "postgres")]
mod feeds;
#[cfg(feature = "postgres")]
mod clusters;

#[cfg(feature = "postgres")]
use crate::error::AtomicCoreError;
#[cfg(feature = "postgres")]
use crate::storage::traits::*;
#[cfg(feature = "postgres")]
use async_trait::async_trait;
#[cfg(feature = "postgres")]
use sqlx::PgPool;

/// Postgres-backed storage implementation using sqlx + pgvector.
#[cfg(feature = "postgres")]
#[derive(Clone)]
pub struct PostgresStorage {
    pub(crate) pool: PgPool,
}

#[cfg(feature = "postgres")]
impl PostgresStorage {
    /// Connect to a Postgres database.
    pub async fn connect(database_url: &str) -> Result<Self, AtomicCoreError> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(format!("Postgres connection failed: {}", e)))?;
        Ok(Self { pool })
    }

    /// Run migrations to set up the schema.
    async fn run_migrations(&self) -> Result<(), AtomicCoreError> {
        let migration_sql = include_str!("migrations/001_initial.sql");

        // Check if schema already exists
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'schema_version')"
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(false);

        if !exists {
            sqlx::raw_sql(migration_sql)
                .execute(&self.pool)
                .await
                .map_err(|e| AtomicCoreError::DatabaseOperation(format!("Migration failed: {}", e)))?;
        }

        Ok(())
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl Storage for PostgresStorage {
    async fn initialize(&self) -> StorageResult<()> {
        self.run_migrations().await
    }

    async fn shutdown(&self) -> StorageResult<()> {
        self.pool.close().await;
        Ok(())
    }

    fn storage_path(&self) -> &std::path::Path {
        // Postgres doesn't have a file path; return a placeholder
        std::path::Path::new("postgres")
    }
}
