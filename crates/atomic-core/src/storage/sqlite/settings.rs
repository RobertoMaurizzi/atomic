use std::collections::HashMap;

use super::SqliteStorage;
use crate::error::AtomicCoreError;
use crate::storage::traits::*;
use crate::tokens::ApiTokenInfo;
use async_trait::async_trait;

// ==================== Settings ====================

impl SqliteStorage {
    pub(crate) fn get_all_settings_sync(&self) -> StorageResult<HashMap<String, String>> {
        let conn = self.db.read_conn()?;
        crate::settings::get_all_settings(&conn)
    }

    pub(crate) fn get_setting_sync(&self, key: &str) -> StorageResult<Option<String>> {
        let conn = self.db.read_conn()?;
        match crate::settings::get_setting(&conn, key) {
            Ok(value) => Ok(Some(value)),
            Err(_) => Ok(None),
        }
    }

    pub(crate) fn set_setting_sync(&self, key: &str, value: &str) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::settings::set_setting(&conn, key, value)
    }
}

#[async_trait]
impl SettingsStore for SqliteStorage {
    async fn get_all_settings(&self) -> StorageResult<HashMap<String, String>> {
        self.get_all_settings_sync()
    }

    async fn get_setting(&self, key: &str) -> StorageResult<Option<String>> {
        self.get_setting_sync(key)
    }

    async fn set_setting(&self, key: &str, value: &str) -> StorageResult<()> {
        self.set_setting_sync(key, value)
    }
}

// ==================== Tokens ====================

impl SqliteStorage {
    pub(crate) fn create_api_token_sync(
        &self,
        name: &str,
    ) -> StorageResult<(ApiTokenInfo, String)> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::tokens::create_token(&conn, name)
    }

    pub(crate) fn list_api_tokens_sync(&self) -> StorageResult<Vec<ApiTokenInfo>> {
        let conn = self.db.read_conn()?;
        crate::tokens::list_tokens(&conn)
    }

    pub(crate) fn verify_api_token_sync(
        &self,
        raw_token: &str,
    ) -> StorageResult<Option<ApiTokenInfo>> {
        let conn = self.db.read_conn()?;
        crate::tokens::verify_token(&conn, raw_token)
    }

    pub(crate) fn revoke_api_token_sync(&self, id: &str) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::tokens::revoke_token(&conn, id)
    }

    pub(crate) fn update_token_last_used_sync(&self, id: &str) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::tokens::update_last_used(&conn, id)
    }

    pub(crate) fn migrate_legacy_token_sync(&self) -> StorageResult<bool> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::tokens::migrate_legacy_token(&conn)
    }

    pub(crate) fn ensure_default_token_sync(
        &self,
    ) -> StorageResult<Option<(ApiTokenInfo, String)>> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::tokens::ensure_default_token(&conn)
    }
}

#[async_trait]
impl TokenStore for SqliteStorage {
    async fn create_api_token(
        &self,
        name: &str,
    ) -> StorageResult<(ApiTokenInfo, String)> {
        self.create_api_token_sync(name)
    }

    async fn list_api_tokens(&self) -> StorageResult<Vec<ApiTokenInfo>> {
        self.list_api_tokens_sync()
    }

    async fn verify_api_token(
        &self,
        raw_token: &str,
    ) -> StorageResult<Option<ApiTokenInfo>> {
        self.verify_api_token_sync(raw_token)
    }

    async fn revoke_api_token(&self, id: &str) -> StorageResult<()> {
        self.revoke_api_token_sync(id)
    }

    async fn update_token_last_used(&self, id: &str) -> StorageResult<()> {
        self.update_token_last_used_sync(id)
    }

    async fn migrate_legacy_token(&self) -> StorageResult<bool> {
        self.migrate_legacy_token_sync()
    }

    async fn ensure_default_token(&self) -> StorageResult<Option<(ApiTokenInfo, String)>> {
        self.ensure_default_token_sync()
    }
}
