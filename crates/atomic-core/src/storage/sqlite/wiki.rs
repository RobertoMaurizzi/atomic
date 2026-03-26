use super::SqliteStorage;
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::storage::traits::*;
use crate::wiki;
use async_trait::async_trait;

/// Sync helper methods for wiki operations.
impl SqliteStorage {
    pub(crate) fn get_wiki_sync(
        &self,
        tag_id: &str,
    ) -> StorageResult<Option<WikiArticleWithCitations>> {
        let conn = self.db.read_conn()?;
        wiki::load_wiki_article(&conn, tag_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn get_wiki_status_sync(&self, tag_id: &str) -> StorageResult<WikiArticleStatus> {
        let conn = self.db.read_conn()?;
        wiki::get_article_status(&conn, tag_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn save_wiki_sync(
        &self,
        tag_id: &str,
        content: &str,
        citations: &[WikiCitation],
        atom_count: i32,
    ) -> StorageResult<WikiArticleWithCitations> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();

        let article = WikiArticle {
            id: id.clone(),
            tag_id: tag_id.to_string(),
            content: content.to_string(),
            created_at: now.clone(),
            updated_at: now,
            atom_count,
        };

        // save_wiki_article expects WikiLink slice; when saving via the trait
        // we don't have link extraction context, so pass an empty slice.
        wiki::save_wiki_article(&conn, &article, citations, &[])
            .map_err(|e| AtomicCoreError::Wiki(e))?;

        Ok(WikiArticleWithCitations {
            article,
            citations: citations.to_vec(),
        })
    }

    pub(crate) fn delete_wiki_sync(&self, tag_id: &str) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        wiki::delete_article(&conn, tag_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn get_wiki_links_sync(&self, tag_id: &str) -> StorageResult<Vec<WikiLink>> {
        let conn = self.db.read_conn()?;
        wiki::load_wiki_links(&conn, tag_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn list_wiki_versions_sync(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<WikiVersionSummary>> {
        let conn = self.db.read_conn()?;
        wiki::list_wiki_versions(&conn, tag_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn get_wiki_version_sync(
        &self,
        version_id: &str,
    ) -> StorageResult<Option<WikiArticleVersion>> {
        let conn = self.db.read_conn()?;
        wiki::get_wiki_version(&conn, version_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn get_all_wiki_articles_sync(&self) -> StorageResult<Vec<WikiArticleSummary>> {
        let conn = self.db.read_conn()?;
        wiki::load_all_wiki_articles(&conn).map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn get_suggested_wiki_articles_sync(
        &self,
        limit: i32,
    ) -> StorageResult<Vec<SuggestedArticle>> {
        let conn = self.db.read_conn()?;
        wiki::get_suggested_wiki_articles(&conn, limit).map_err(|e| AtomicCoreError::Wiki(e))
    }
}

#[async_trait]
impl WikiStore for SqliteStorage {
    async fn get_wiki(
        &self,
        tag_id: &str,
    ) -> StorageResult<Option<WikiArticleWithCitations>> {
        let storage = self.clone();
        let tag_id = tag_id.to_string();
        tokio::task::spawn_blocking(move || storage.get_wiki_sync(&tag_id))
            .await
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn get_wiki_status(&self, tag_id: &str) -> StorageResult<WikiArticleStatus> {
        let storage = self.clone();
        let tag_id = tag_id.to_string();
        tokio::task::spawn_blocking(move || storage.get_wiki_status_sync(&tag_id))
            .await
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn save_wiki(
        &self,
        tag_id: &str,
        content: &str,
        citations: &[WikiCitation],
        atom_count: i32,
    ) -> StorageResult<WikiArticleWithCitations> {
        let storage = self.clone();
        let tag_id = tag_id.to_string();
        let content = content.to_string();
        let citations = citations.to_vec();
        tokio::task::spawn_blocking(move || {
            storage.save_wiki_sync(&tag_id, &content, &citations, atom_count)
        })
        .await
        .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn delete_wiki(&self, tag_id: &str) -> StorageResult<()> {
        let storage = self.clone();
        let tag_id = tag_id.to_string();
        tokio::task::spawn_blocking(move || storage.delete_wiki_sync(&tag_id))
            .await
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn get_wiki_links(&self, tag_id: &str) -> StorageResult<Vec<WikiLink>> {
        let storage = self.clone();
        let tag_id = tag_id.to_string();
        tokio::task::spawn_blocking(move || storage.get_wiki_links_sync(&tag_id))
            .await
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn list_wiki_versions(&self, tag_id: &str) -> StorageResult<Vec<WikiVersionSummary>> {
        let storage = self.clone();
        let tag_id = tag_id.to_string();
        tokio::task::spawn_blocking(move || storage.list_wiki_versions_sync(&tag_id))
            .await
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn get_wiki_version(
        &self,
        version_id: &str,
    ) -> StorageResult<Option<WikiArticleVersion>> {
        let storage = self.clone();
        let version_id = version_id.to_string();
        tokio::task::spawn_blocking(move || storage.get_wiki_version_sync(&version_id))
            .await
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn get_all_wiki_articles(&self) -> StorageResult<Vec<WikiArticleSummary>> {
        let storage = self.clone();
        tokio::task::spawn_blocking(move || storage.get_all_wiki_articles_sync())
            .await
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn get_suggested_wiki_articles(
        &self,
        limit: i32,
    ) -> StorageResult<Vec<SuggestedArticle>> {
        let storage = self.clone();
        tokio::task::spawn_blocking(move || storage.get_suggested_wiki_articles_sync(limit))
            .await
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }
}
