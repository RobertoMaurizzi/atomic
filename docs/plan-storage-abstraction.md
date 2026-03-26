# Storage Provider Abstraction Plan

## Status

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 0: Seal the Leaks | **Done** | All SQL removed from wrapper crates, `rusqlite` dropped from their Cargo.tomls |
| Phase 1: Storage Traits + SQLite Impl | **Done** | 8 async traits (74 methods), `SqliteStorage` in 9 files (~3500 LOC), AtomicCore delegates 53 methods |
| Phase 2: Postgres + pgvector Backend | **Not started** | This document |
| Phase 3: Configuration + Wiring | **Not started** | This document |

---

## Phase 2: Postgres + pgvector Backend

**Goal:** Implement `Storage` for Postgres so the same test suite passes against both backends.

### 2.1 Add sqlx dependency

Add to `crates/atomic-core/Cargo.toml`:

```toml
[dependencies]
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio-rustls", "uuid", "chrono", "json"], optional = true }

[features]
default = []
postgres = ["sqlx"]
```

Using a feature flag avoids pulling sqlx into builds that only need SQLite (Tauri desktop, standalone MCP). The server binary enables it:

```toml
# crates/atomic-server/Cargo.toml
atomic-core = { path = "../atomic-core", features = ["openapi", "postgres"] }
```

**Files to modify:**
- `crates/atomic-core/Cargo.toml`
- `crates/atomic-server/Cargo.toml`

### 2.2 Postgres migrations

Create `crates/atomic-core/src/storage/postgres/migrations/` with numbered SQL files. These mirror the SQLite schema in `db.rs` but use Postgres syntax.

**`001_initial.sql`** — Core tables:

```sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE atoms (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL DEFAULT '',
    snippet TEXT NOT NULL DEFAULT '',
    source_url TEXT,
    source TEXT,
    published_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    embedding_status TEXT NOT NULL DEFAULT 'pending',
    tagging_status TEXT NOT NULL DEFAULT 'pending'
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    atom_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE atom_tags (
    atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (atom_id, tag_id)
);

CREATE TABLE atom_chunks (
    id TEXT PRIMARY KEY,
    atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    embedding vector,          -- pgvector column, dimension set at insert time
    token_count INTEGER DEFAULT 0
);

CREATE TABLE atom_positions (
    atom_id TEXT PRIMARY KEY REFERENCES atoms(id) ON DELETE CASCADE,
    x REAL NOT NULL,
    y REAL NOT NULL
);

CREATE TABLE semantic_edges (
    id TEXT PRIMARY KEY,
    source_atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
    target_atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
    similarity_score REAL NOT NULL,
    source_chunk_index INTEGER,
    target_chunk_index INTEGER,
    created_at TEXT NOT NULL
);

CREATE TABLE atom_clusters (
    atom_id TEXT NOT NULL,
    cluster_id INTEGER NOT NULL,
    PRIMARY KEY (atom_id)
);

CREATE TABLE tag_embeddings (
    tag_id TEXT PRIMARY KEY REFERENCES tags(id) ON DELETE CASCADE,
    embedding vector
);

-- Wiki
CREATE TABLE wiki_articles (
    id TEXT PRIMARY KEY,
    tag_id TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    atom_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE wiki_citations (
    id TEXT PRIMARY KEY,
    wiki_article_id TEXT NOT NULL REFERENCES wiki_articles(id) ON DELETE CASCADE,
    citation_index INTEGER NOT NULL,
    atom_id TEXT NOT NULL,
    chunk_index INTEGER,
    excerpt TEXT NOT NULL DEFAULT ''
);

CREATE TABLE wiki_links (
    id TEXT PRIMARY KEY,
    source_article_id TEXT NOT NULL REFERENCES wiki_articles(id) ON DELETE CASCADE,
    target_tag_id TEXT NOT NULL,
    link_text TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE wiki_article_versions (
    id TEXT PRIMARY KEY,
    tag_id TEXT NOT NULL,
    content TEXT NOT NULL,
    atom_count INTEGER NOT NULL DEFAULT 0,
    version_number INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

-- Chat
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    title TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    is_archived INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE conversation_tags (
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (conversation_id, tag_id)
);

CREATE TABLE chat_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    message_index INTEGER NOT NULL
);

CREATE TABLE chat_tool_calls (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
    tool_name TEXT NOT NULL,
    tool_input TEXT NOT NULL DEFAULT '{}',
    tool_result TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE chat_citations (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
    atom_id TEXT NOT NULL,
    chunk_index INTEGER,
    excerpt TEXT NOT NULL DEFAULT '',
    relevance_score REAL
);

-- Feeds
CREATE TABLE feeds (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    title TEXT,
    site_url TEXT,
    poll_interval INTEGER NOT NULL DEFAULT 3600,
    last_polled_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL,
    is_paused INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE feed_tags (
    feed_id TEXT NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (feed_id, tag_id)
);

CREATE TABLE feed_items (
    feed_id TEXT NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    guid TEXT NOT NULL,
    atom_id TEXT,
    seen_at TEXT NOT NULL,
    skipped INTEGER NOT NULL DEFAULT 0,
    skip_reason TEXT,
    PRIMARY KEY (feed_id, guid)
);

-- Settings (per-database, NOT shared — registry handles shared settings)
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Full-text search: tsvector column + GIN index
ALTER TABLE atom_chunks ADD COLUMN content_tsv tsvector
    GENERATED ALWAYS AS (to_tsvector('english', content)) STORED;
CREATE INDEX idx_atom_chunks_fts ON atom_chunks USING GIN(content_tsv);

-- Vector indexes (IVFFlat for large datasets; switch to HNSW if <100K rows)
CREATE INDEX idx_atom_chunks_embedding ON atom_chunks USING ivfflat (embedding vector_cosine_ops);
CREATE INDEX idx_tag_embeddings_embedding ON tag_embeddings USING ivfflat (embedding vector_cosine_ops);

-- Schema version tracking
CREATE TABLE schema_version (
    version INTEGER NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
INSERT INTO schema_version (version) VALUES (1);
```

Also create all the same indexes from the SQLite schema (the `CREATE INDEX` statements in `db.rs` lines 380-399, 430, 473-474, 541-542, 562).

**Files to create:**
- `crates/atomic-core/src/storage/postgres/migrations/001_initial.sql`

### 2.3 PostgresStorage struct

Create `crates/atomic-core/src/storage/postgres/mod.rs`:

```rust
#[cfg(feature = "postgres")]
pub mod atoms;
#[cfg(feature = "postgres")]
pub mod tags;
#[cfg(feature = "postgres")]
pub mod chunks;
#[cfg(feature = "postgres")]
pub mod search;
#[cfg(feature = "postgres")]
pub mod chat;
#[cfg(feature = "postgres")]
pub mod wiki;
#[cfg(feature = "postgres")]
pub mod feeds;
#[cfg(feature = "postgres")]
pub mod clusters;

use sqlx::PgPool;

#[derive(Clone)]
pub struct PostgresStorage {
    pool: PgPool,
}

impl PostgresStorage {
    pub async fn connect(database_url: &str) -> Result<Self, AtomicCoreError> {
        let pool = PgPool::connect(database_url).await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        Ok(Self { pool })
    }
}
```

**Files to create:**
- `crates/atomic-core/src/storage/postgres/mod.rs`
- `crates/atomic-core/src/storage/postgres/atoms.rs`
- `crates/atomic-core/src/storage/postgres/tags.rs`
- `crates/atomic-core/src/storage/postgres/chunks.rs`
- `crates/atomic-core/src/storage/postgres/search.rs`
- `crates/atomic-core/src/storage/postgres/chat.rs`
- `crates/atomic-core/src/storage/postgres/wiki.rs`
- `crates/atomic-core/src/storage/postgres/feeds.rs`
- `crates/atomic-core/src/storage/postgres/clusters.rs`

### 2.4 Implementation order and SQLite → Postgres translation guide

Implement one trait at a time, running tests after each. Order by complexity:

**Step 1: `Storage` supertrait + `AtomStore`** (straightforward CRUD)

Key translations:
| SQLite | Postgres |
|--------|----------|
| `?1, ?2` | `$1, $2` |
| `INSERT OR REPLACE` | `INSERT ... ON CONFLICT (pk) DO UPDATE SET ...` |
| `INSERT OR IGNORE` | `INSERT ... ON CONFLICT DO NOTHING` |
| `rusqlite::params![]` | `sqlx::query().bind()` chains |
| `conn.query_row(sql, params, \|row\| ...)` | `sqlx::query_as(sql).bind(...).fetch_one(&pool)` |
| `stmt.query_map(params, \|row\| ...)` | `sqlx::query_as(sql).bind(...).fetch_all(&pool)` |
| `conn.execute(sql, params)` | `sqlx::query(sql).bind(...).execute(&pool)` |
| `COALESCE(embedding_status, 'pending')` | Same (standard SQL) |

The `atom_from_row` helper becomes a `sqlx::FromRow` derive or manual `Row` mapping.

**Step 2: `TagStore`** (recursive CTEs work in Postgres)

Key translations:
| SQLite | Postgres |
|--------|----------|
| `WITH RECURSIVE` | Same syntax, works identically |
| `GROUP_CONCAT` | `STRING_AGG` |
| Implicit type coercion | May need explicit `::TEXT` or `::INTEGER` casts |

**Step 3: `SearchStore`** (biggest divergence)

Key translations:
| SQLite | Postgres |
|--------|----------|
| `vec_chunks WHERE embedding MATCH ?1` (sqlite-vec) | `ORDER BY embedding <-> $1::vector LIMIT $2` (pgvector L2) or `<=>` (cosine) |
| `distance` from vec_chunks | `embedding <-> $1::vector` expression |
| `1.0 - (distance * distance / 2.0)` | `1 - (embedding <=> $1::vector)` (cosine distance → similarity) |
| FTS5 `MATCH ?1` + `bm25()` | `content_tsv @@ plainto_tsquery('english', $1)` + `ts_rank(content_tsv, ...)` |
| `atom_chunks_fts` virtual table | `content_tsv` tsvector column on `atom_chunks` |

The hybrid search (Reciprocal Rank Fusion) logic stays in AtomicCore — only the two individual search methods need Postgres translations.

**Step 4: `ChunkStore`** (embedding storage)

Key translations:
| SQLite | Postgres |
|--------|----------|
| `f32` little-endian BLOB in `vec_chunks` | pgvector `vector` type, pass as `pgvector::Vector` or `&[f32]` |
| `INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?, ?)` | `INSERT INTO atom_chunks (..., embedding) VALUES (..., $N::vector)` |
| Separate `vec_chunks` + `atom_chunks` tables | Single `atom_chunks` table with `embedding vector` column |
| `atom_chunks_fts` (FTS5 virtual table) | `content_tsv` generated tsvector column (auto-updated) |

Note: Postgres unifies chunks + embeddings + FTS into one table. SQLite uses 3 separate structures (atom_chunks, vec_chunks, atom_chunks_fts). The trait interface hides this difference.

**Step 5: `ChatStore`, `WikiStore`** (mostly standard SQL)

Minimal translation needed — mostly parameter syntax (`?N` → `$N`) and upsert syntax.

**Step 6: `FeedStore`** (standard SQL)

Same as above. `INSERT OR IGNORE` → `INSERT ... ON CONFLICT DO NOTHING`.

**Step 7: `ClusterStore`** (compute in Rust, store in Postgres)

Cluster computation happens in Rust (reading embeddings, running algorithm). Only the storage/retrieval of cluster assignments needs translation.

### 2.5 Parameterized test suite

Create `crates/atomic-core/tests/storage_tests.rs` with backend-parameterized tests.

```rust
use atomic_core::storage::{Storage, SqliteStorage};
#[cfg(feature = "postgres")]
use atomic_core::storage::postgres::PostgresStorage;

enum Backend { Sqlite, Postgres }

async fn create_test_storage(backend: Backend) -> Box<dyn Storage> {
    match backend {
        Backend::Sqlite => {
            // In-memory or tempdir SQLite
            let db = Database::open_or_create(tempdir.path().join("test.db")).unwrap();
            Box::new(SqliteStorage::new(Arc::new(db)))
        }
        Backend::Postgres => {
            // Requires ATOMIC_TEST_DATABASE_URL env var
            let url = std::env::var("ATOMIC_TEST_DATABASE_URL")
                .expect("Set ATOMIC_TEST_DATABASE_URL for Postgres tests");
            let storage = PostgresStorage::connect(&url).await.unwrap();
            storage.initialize().await.unwrap();
            Box::new(storage)
        }
    }
}

// Each test runs against both backends:
#[tokio::test]
async fn test_create_and_get_atom_sqlite() {
    test_create_and_get_atom(Backend::Sqlite).await;
}

#[tokio::test]
#[cfg(feature = "postgres")]
async fn test_create_and_get_atom_postgres() {
    test_create_and_get_atom(Backend::Postgres).await;
}

async fn test_create_and_get_atom(backend: Backend) {
    let storage = create_test_storage(backend).await;
    // ... test body using storage trait methods ...
}
```

Every trait method gets at least one parameterized test. These tests catch behavioral divergence between backends.

**Files to create:**
- `crates/atomic-core/tests/storage_tests.rs`

### 2.6 Docker Compose for Postgres test environment

Create `docker-compose.test.yml`:

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_DB: atomic_test
      POSTGRES_USER: atomic
      POSTGRES_PASSWORD: atomic_test
    ports:
      - "5433:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U atomic"]
      interval: 2s
      timeout: 5s
      retries: 5
```

Usage:
```bash
docker compose -f docker-compose.test.yml up -d
export ATOMIC_TEST_DATABASE_URL="postgres://atomic:atomic_test@localhost:5433/atomic_test"
cargo test --workspace --features postgres
docker compose -f docker-compose.test.yml down
```

**Files to create:**
- `docker-compose.test.yml`

### 2.7 Verification

1. `cargo test --workspace` — all 184 existing tests still pass (SQLite path unchanged)
2. `cargo test --workspace --features postgres` with Postgres running — parameterized tests pass on both backends
3. Every trait method has at least one passing test on Postgres

---

## Phase 3: Configuration + Wiring

**Goal:** Make the backend selectable at runtime via CLI args and env vars.

### 3.1 Add storage CLI args

Update `crates/atomic-server/src/config.rs`:

```rust
#[derive(Subcommand, Debug)]
pub enum Command {
    Serve {
        #[arg(long, default_value_t = 8080)]
        port: u16,

        #[arg(long, default_value = "127.0.0.1")]
        bind: String,

        #[arg(long, env = "PUBLIC_URL")]
        public_url: Option<String>,

        /// Storage backend: "sqlite" (default) or "postgres"
        #[arg(long, default_value = "sqlite", env = "ATOMIC_STORAGE")]
        storage: String,

        /// Postgres connection string (required when --storage=postgres).
        /// Example: postgres://user:pass@localhost:5432/atomic
        #[arg(long, env = "ATOMIC_DATABASE_URL")]
        database_url: Option<String>,
    },
    // ...
}
```

**Files to modify:**
- `crates/atomic-server/src/config.rs`

### 3.2 Update DatabaseManager for Postgres

Add a Postgres-aware constructor to `DatabaseManager`. The registry (settings, tokens, OAuth, DB list) stays SQLite-only — it's inherently local config. Only data databases switch to Postgres.

```rust
// crates/atomic-core/src/manager.rs
impl DatabaseManager {
    /// Create a manager that uses Postgres for data storage.
    /// Registry stays as local SQLite.
    #[cfg(feature = "postgres")]
    pub async fn new_postgres(
        data_dir: impl AsRef<Path>,
        database_url: &str,
    ) -> Result<Self, AtomicCoreError> {
        let registry = Arc::new(Registry::open_or_create(&data_dir)?);
        let storage = PostgresStorage::connect(database_url).await?;
        storage.initialize().await?;
        // ... construct AtomicCore with Postgres storage ...
    }
}
```

AtomicCore constructor needs a Postgres path:

```rust
impl AtomicCore {
    #[cfg(feature = "postgres")]
    pub async fn open_postgres(
        database_url: &str,
        registry: Option<Arc<Registry>>,
    ) -> Result<Self, AtomicCoreError> {
        let storage = PostgresStorage::connect(database_url).await?;
        storage.initialize().await?;
        // ... construct Self with storage ...
    }
}
```

**Key decision:** AtomicCore currently holds both `db: Arc<Database>` and `storage: SqliteStorage`. For Postgres, it would hold a `PostgresStorage` instead. To support both at runtime, AtomicCore needs to either:

- **Option A:** Hold `Box<dyn Storage>` (requires making AtomicCore methods async) — cleanest but biggest API change
- **Option B:** Hold an enum `StorageBackend { Sqlite(SqliteStorage), Postgres(PostgresStorage) }` and match in each method — no async change but verbose
- **Option C:** Hold `Box<dyn Storage>` but keep sync methods that use `tokio::runtime::Handle::current().block_on()` — pragmatic but panics if called outside tokio

**Recommended: Option A.** Make AtomicCore's public DB methods async. The callers (actix-web handlers, Tauri commands) are already async. This is the natural progression from Phase 1's sync-wrapped design.

The migration path:
1. Change `AtomicCore` to hold `Box<dyn Storage>` (or `Arc<dyn Storage>`)
2. Make delegated methods async (add `async` keyword, callers add `.await`)
3. SqliteStorage's async trait methods use `spawn_blocking` around the sync helpers
4. PostgresStorage's async trait methods are natively async (sqlx)

**Files to modify:**
- `crates/atomic-core/src/lib.rs` — AtomicCore struct + all delegated methods become async
- `crates/atomic-core/src/manager.rs` — add Postgres constructor
- `crates/atomic-server/src/routes/*.rs` — add `.await` to AtomicCore calls
- `crates/atomic-server/src/main.rs` — storage backend selection at startup
- `crates/atomic-mcp/src/server.rs` — add `.await` to AtomicCore calls
- `src-tauri/src/*.rs` — add `.await` to AtomicCore calls (Tauri commands are already async)

### 3.3 Server startup wiring

Update `crates/atomic-server/src/main.rs` (or wherever the server starts) to branch on the storage flag:

```rust
let manager = match serve_args.storage.as_str() {
    "postgres" => {
        let url = serve_args.database_url
            .as_deref()
            .ok_or("--database-url required when --storage=postgres")?;
        Arc::new(DatabaseManager::new_postgres(&data_dir, url).await?)
    }
    "sqlite" | _ => {
        Arc::new(DatabaseManager::new(&data_dir)?)
    }
};
```

### 3.4 Docker Compose for production Postgres

Create `docker-compose.yml` example:

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_DB: atomic
      POSTGRES_USER: atomic
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:-changeme}
    volumes:
      - pgdata:/var/lib/postgresql/data
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U atomic"]
      interval: 5s
      timeout: 5s
      retries: 5

  atomic:
    build: .
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      ATOMIC_STORAGE: postgres
      ATOMIC_DATABASE_URL: postgres://atomic:${POSTGRES_PASSWORD:-changeme}@postgres:5432/atomic
    ports:
      - "8080:8080"

volumes:
  pgdata:
```

**Files to create:**
- `docker-compose.yml`

### 3.5 Verification

1. `cargo test --workspace` — SQLite path unchanged, all tests pass
2. `atomic-server serve --storage sqlite` — works as before
3. `atomic-server serve --storage postgres --database-url postgres://...` — starts against Postgres
4. Full round-trip: create atoms via API → embedding pipeline runs → search returns results → wiki generates → chat works — all against Postgres backend
5. Error case: `--storage postgres` without `--database-url` prints helpful error

---

## Execution Order

| Step | What | Depends On |
|------|------|------------|
| 2.1 | Add sqlx dep with `postgres` feature | — |
| 2.2 | Write Postgres migration SQL | — |
| 2.3 | Create PostgresStorage struct | 2.1 |
| 2.4.1 | Implement `AtomStore` for Postgres | 2.2, 2.3 |
| 2.4.2 | Implement `TagStore` for Postgres | 2.4.1 |
| 2.4.3 | Implement `SearchStore` for Postgres | 2.4.1 |
| 2.4.4 | Implement `ChunkStore` for Postgres | 2.4.1 |
| 2.4.5 | Implement `ChatStore` for Postgres | 2.4.1 |
| 2.4.6 | Implement `WikiStore` for Postgres | 2.4.1 |
| 2.4.7 | Implement `FeedStore` + `ClusterStore` for Postgres | 2.4.1 |
| 2.5 | Write parameterized test suite | 2.4.* |
| 2.6 | Docker Compose for test Postgres | — |
| 2.7 | Verify all tests pass on both backends | 2.5, 2.6 |
| 3.1 | Add CLI args (`--storage`, `--database-url`) | — |
| 3.2 | Update DatabaseManager + AtomicCore for Postgres | 2.7 |
| 3.3 | Wire up server startup branching | 3.1, 3.2 |
| 3.4 | Production Docker Compose | 3.3 |
| 3.5 | End-to-end verification | 3.3 |

Steps 2.1, 2.2, 2.6, and 3.1 can be done in parallel. Steps 2.4.1–2.4.7 can be parallelized once the struct and migration are in place.

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| sqlx compile-time overhead | Slower builds | Feature-gated (`postgres` feature), only atomic-server enables it |
| Postgres query semantics diverge (NULLs, collation, type coercion) | Silent data differences | Parameterized tests catch these; run both backends in CI |
| pgvector index build on large datasets | Slow initial startup | Use `CREATE INDEX CONCURRENTLY` in migrations; IVFFlat needs `VACUUM` to build properly |
| Making AtomicCore async (Phase 3.2) | Breaks callers | Mechanical change — add `.await` everywhere. All callers are already async contexts. |
| Connection pool exhaustion under load | Request timeouts | sqlx PgPool defaults to 10 connections; tune `max_connections` based on server config |
| Embedding dimension mismatch | Vector search fails | Validate dimension at startup (compare settings table with Postgres vector column dimension) |
