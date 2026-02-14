//! API token management for authentication
//!
//! Provides named, revocable API tokens backed by SHA-256 hashes in SQLite.
//! Token format: `at_` prefix + 32 random bytes base64url-encoded (~47 chars).

use crate::error::AtomicCoreError;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Token metadata returned to callers (never contains the raw token or hash)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTokenInfo {
    pub id: String,
    pub name: String,
    pub token_prefix: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub is_revoked: bool,
}

/// Generate a raw API token: `at_` + 32 random bytes base64url-encoded
fn generate_raw_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("at_{}", URL_SAFE_NO_PAD.encode(bytes))
}

/// SHA-256 hex digest of a raw token
fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract the display prefix from a raw token (first 10 chars)
fn token_prefix(raw: &str) -> String {
    raw.chars().take(10).collect()
}

/// Create a new named API token. Returns metadata + the raw token (shown once).
pub fn create_token(
    conn: &Connection,
    name: &str,
) -> Result<(ApiTokenInfo, String), AtomicCoreError> {
    let id = Uuid::new_v4().to_string();
    let raw = generate_raw_token();
    let hash = hash_token(&raw);
    let prefix = token_prefix(&raw);
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO api_tokens (id, name, token_hash, token_prefix, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        (&id, name, &hash, &prefix, &now),
    )?;

    let info = ApiTokenInfo {
        id,
        name: name.to_string(),
        token_prefix: prefix,
        created_at: now,
        last_used_at: None,
        is_revoked: false,
    };

    Ok((info, raw))
}

/// List all API tokens (metadata only, including revoked).
pub fn list_tokens(conn: &Connection) -> Result<Vec<ApiTokenInfo>, AtomicCoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, token_prefix, created_at, last_used_at, is_revoked
         FROM api_tokens ORDER BY created_at DESC",
    )?;

    let tokens = stmt
        .query_map([], |row| {
            Ok(ApiTokenInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                token_prefix: row.get(2)?,
                created_at: row.get(3)?,
                last_used_at: row.get(4)?,
                is_revoked: row.get::<_, i32>(5)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tokens)
}

/// Verify a raw token. Returns token info if valid and not revoked, None otherwise.
pub fn verify_token(
    conn: &Connection,
    raw_token: &str,
) -> Result<Option<ApiTokenInfo>, AtomicCoreError> {
    let hash = hash_token(raw_token);

    let result = conn.query_row(
        "SELECT id, name, token_prefix, created_at, last_used_at, is_revoked
         FROM api_tokens WHERE token_hash = ?1 AND is_revoked = 0",
        [&hash],
        |row| {
            Ok(ApiTokenInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                token_prefix: row.get(2)?,
                created_at: row.get(3)?,
                last_used_at: row.get(4)?,
                is_revoked: row.get::<_, i32>(5)? != 0,
            })
        },
    );

    match result {
        Ok(info) => Ok(Some(info)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AtomicCoreError::Database(e)),
    }
}

/// Revoke a token by ID (soft delete).
pub fn revoke_token(conn: &Connection, token_id: &str) -> Result<(), AtomicCoreError> {
    let updated = conn.execute(
        "UPDATE api_tokens SET is_revoked = 1 WHERE id = ?1",
        [token_id],
    )?;

    if updated == 0 {
        return Err(AtomicCoreError::NotFound(format!(
            "API token '{}'",
            token_id
        )));
    }

    Ok(())
}

/// Update the last_used_at timestamp for a token.
pub fn update_last_used(conn: &Connection, token_id: &str) -> Result<(), AtomicCoreError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE api_tokens SET last_used_at = ?1 WHERE id = ?2",
        [&now, token_id],
    )?;
    Ok(())
}

/// Migrate legacy `server_auth_token` from settings to the api_tokens table.
/// Returns true if migration occurred, false if not needed.
pub fn migrate_legacy_token(conn: &Connection) -> Result<bool, AtomicCoreError> {
    // Check if legacy token exists in settings
    let legacy_token: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'server_auth_token'",
            [],
            |row| row.get(0),
        )
        .ok();

    let legacy_token = match legacy_token {
        Some(t) if !t.is_empty() => t,
        _ => return Ok(false),
    };

    // Only migrate if no api_tokens exist yet
    let token_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM api_tokens",
        [],
        |row| row.get(0),
    )?;

    if token_count > 0 {
        return Ok(false);
    }

    // Hash the existing UUID token and insert as a migrated token
    let id = Uuid::new_v4().to_string();
    let hash = hash_token(&legacy_token);
    let prefix = token_prefix(&legacy_token);
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO api_tokens (id, name, token_hash, token_prefix, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        (&id, "default (migrated)", &hash, &prefix, &now),
    )?;

    // Remove the legacy setting
    conn.execute(
        "DELETE FROM settings WHERE key = 'server_auth_token'",
        [],
    )?;

    Ok(true)
}

/// Ensure at least one token exists. If no tokens exist, creates a "default" token.
/// Returns Some with the token info + raw token if created, None if tokens already exist.
pub fn ensure_default_token(
    conn: &Connection,
) -> Result<Option<(ApiTokenInfo, String)>, AtomicCoreError> {
    let token_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM api_tokens",
        [],
        |row| row.get(0),
    )?;

    if token_count > 0 {
        return Ok(None);
    }

    let (info, raw) = create_token(conn, "default")?;
    Ok(Some((info, raw)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::NamedTempFile;

    fn test_conn() -> (Connection, NamedTempFile) {
        let temp = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp.path()).unwrap();
        let conn = db.conn.into_inner().unwrap();
        (conn, temp)
    }

    #[test]
    fn test_create_and_verify_token() {
        let (conn, _tmp) = test_conn();

        let (info, raw) = create_token(&conn, "test token").unwrap();
        assert_eq!(info.name, "test token");
        assert!(!info.is_revoked);
        assert!(raw.starts_with("at_"));
        assert!(raw.len() > 10);
        assert_eq!(info.token_prefix, &raw[..10]);

        // Verify with the raw token
        let verified = verify_token(&conn, &raw).unwrap();
        assert!(verified.is_some());
        assert_eq!(verified.unwrap().id, info.id);
    }

    #[test]
    fn test_verify_wrong_token_returns_none() {
        let (conn, _tmp) = test_conn();

        create_token(&conn, "my token").unwrap();

        let result = verify_token(&conn, "at_totally_wrong_token").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_revoke_then_verify_fails() {
        let (conn, _tmp) = test_conn();

        let (info, raw) = create_token(&conn, "to revoke").unwrap();

        // Verify works before revoke
        assert!(verify_token(&conn, &raw).unwrap().is_some());

        // Revoke
        revoke_token(&conn, &info.id).unwrap();

        // Verify fails after revoke
        assert!(verify_token(&conn, &raw).unwrap().is_none());
    }

    #[test]
    fn test_revoke_nonexistent_token_returns_not_found() {
        let (conn, _tmp) = test_conn();

        let result = revoke_token(&conn, "nonexistent-id");
        assert!(result.is_err());
        match result.unwrap_err() {
            AtomicCoreError::NotFound(_) => {}
            other => panic!("Expected NotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_list_tokens() {
        let (conn, _tmp) = test_conn();

        create_token(&conn, "token A").unwrap();
        create_token(&conn, "token B").unwrap();

        let tokens = list_tokens(&conn).unwrap();
        assert_eq!(tokens.len(), 2);

        let names: Vec<&str> = tokens.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"token A"));
        assert!(names.contains(&"token B"));
    }

    #[test]
    fn test_list_includes_revoked() {
        let (conn, _tmp) = test_conn();

        let (info, _) = create_token(&conn, "will revoke").unwrap();
        create_token(&conn, "stays active").unwrap();
        revoke_token(&conn, &info.id).unwrap();

        let tokens = list_tokens(&conn).unwrap();
        assert_eq!(tokens.len(), 2);

        let revoked = tokens.iter().find(|t| t.name == "will revoke").unwrap();
        assert!(revoked.is_revoked);

        let active = tokens.iter().find(|t| t.name == "stays active").unwrap();
        assert!(!active.is_revoked);
    }

    #[test]
    fn test_update_last_used() {
        let (conn, _tmp) = test_conn();

        let (info, _) = create_token(&conn, "test").unwrap();
        assert!(info.last_used_at.is_none());

        update_last_used(&conn, &info.id).unwrap();

        let tokens = list_tokens(&conn).unwrap();
        let updated = tokens.iter().find(|t| t.id == info.id).unwrap();
        assert!(updated.last_used_at.is_some());
    }

    #[test]
    fn test_migrate_legacy_token() {
        let (conn, _tmp) = test_conn();

        // Insert a legacy token in settings
        let legacy_uuid = "550e8400-e29b-41d4-a716-446655440000";
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('server_auth_token', ?1)",
            [legacy_uuid],
        )
        .unwrap();

        // Migrate
        let migrated = migrate_legacy_token(&conn).unwrap();
        assert!(migrated);

        // The old UUID should still verify
        let verified = verify_token(&conn, legacy_uuid).unwrap();
        assert!(verified.is_some());
        assert_eq!(verified.unwrap().name, "default (migrated)");

        // Legacy setting should be gone
        let setting: Result<String, _> = conn.query_row(
            "SELECT value FROM settings WHERE key = 'server_auth_token'",
            [],
            |row| row.get(0),
        );
        assert!(setting.is_err());
    }

    #[test]
    fn test_migrate_legacy_noop_when_tokens_exist() {
        let (conn, _tmp) = test_conn();

        // Create a token first
        create_token(&conn, "existing").unwrap();

        // Insert a legacy token
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('server_auth_token', 'some-uuid')",
            [],
        )
        .unwrap();

        // Migration should be a no-op
        let migrated = migrate_legacy_token(&conn).unwrap();
        assert!(!migrated);
    }

    #[test]
    fn test_migrate_legacy_noop_when_no_setting() {
        let (conn, _tmp) = test_conn();

        let migrated = migrate_legacy_token(&conn).unwrap();
        assert!(!migrated);
    }

    #[test]
    fn test_ensure_default_token_creates_on_empty() {
        let (conn, _tmp) = test_conn();

        let result = ensure_default_token(&conn).unwrap();
        assert!(result.is_some());

        let (info, raw) = result.unwrap();
        assert_eq!(info.name, "default");
        assert!(raw.starts_with("at_"));
    }

    #[test]
    fn test_ensure_default_token_noop_when_tokens_exist() {
        let (conn, _tmp) = test_conn();

        create_token(&conn, "already here").unwrap();

        let result = ensure_default_token(&conn).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_token_format() {
        let raw = generate_raw_token();
        assert!(raw.starts_with("at_"));
        // at_ (3 chars) + base64url of 32 bytes = 43 chars = 46 total
        assert_eq!(raw.len(), 46, "Token should be 46 chars, got {}: {}", raw.len(), raw);
    }

    #[test]
    fn test_token_uniqueness() {
        let t1 = generate_raw_token();
        let t2 = generate_raw_token();
        assert_ne!(t1, t2);
    }
}
