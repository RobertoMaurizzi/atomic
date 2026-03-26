use super::PostgresStorage;
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::storage::traits::*;
use async_trait::async_trait;

#[async_trait]
impl WikiStore for PostgresStorage {
    async fn get_wiki(
        &self,
        tag_id: &str,
    ) -> StorageResult<Option<WikiArticleWithCitations>> {
        // Get article
        let article_row = sqlx::query_as::<_, (String, String, String, String, String, i32)>(
            "SELECT id, tag_id, content, created_at, updated_at, atom_count
             FROM wiki_articles WHERE tag_id = $1",
        )
        .bind(tag_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let article = match article_row {
            Some((id, tag_id, content, created_at, updated_at, atom_count)) => WikiArticle {
                id,
                tag_id,
                content,
                created_at,
                updated_at,
                atom_count,
            },
            None => return Ok(None),
        };

        // Get citations
        let citation_rows = sqlx::query_as::<_, (String, i32, String, Option<i32>, String)>(
            "SELECT id, citation_index, atom_id, chunk_index, excerpt
             FROM wiki_citations
             WHERE wiki_article_id = $1
             ORDER BY citation_index",
        )
        .bind(&article.id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let citations: Vec<WikiCitation> = citation_rows
            .into_iter()
            .map(|(id, citation_index, atom_id, chunk_index, excerpt)| WikiCitation {
                id,
                citation_index,
                atom_id,
                chunk_index,
                excerpt,
            })
            .collect();

        Ok(Some(WikiArticleWithCitations { article, citations }))
    }

    async fn get_wiki_status(&self, tag_id: &str) -> StorageResult<WikiArticleStatus> {
        // Count distinct atoms across this tag and all descendants using recursive CTE
        let current_atom_count: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
            "WITH RECURSIVE descendant_tags(id) AS (
                SELECT $1::text
                UNION ALL
                SELECT t.id FROM tags t
                INNER JOIN descendant_tags dt ON t.parent_id = dt.id
            )
            SELECT COUNT(DISTINCT atom_id) FROM atom_tags
            WHERE tag_id IN (SELECT id FROM descendant_tags)",
        )
        .bind(tag_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        let current_atom_count = current_atom_count.unwrap_or(0);

        // Get article info if exists
        let article_info = sqlx::query_as::<_, (i32, String)>(
            "SELECT atom_count, updated_at FROM wiki_articles WHERE tag_id = $1",
        )
        .bind(tag_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        match article_info {
            Some((article_atom_count, updated_at)) => {
                let new_atoms = (current_atom_count as i32 - article_atom_count).max(0);
                Ok(WikiArticleStatus {
                    has_article: true,
                    article_atom_count,
                    current_atom_count: current_atom_count as i32,
                    new_atoms_available: new_atoms,
                    updated_at: Some(updated_at),
                })
            }
            None => Ok(WikiArticleStatus {
                has_article: false,
                article_atom_count: 0,
                current_atom_count: current_atom_count as i32,
                new_atoms_available: 0,
                updated_at: None,
            }),
        }
    }

    async fn save_wiki(
        &self,
        tag_id: &str,
        content: &str,
        citations: &[WikiCitation],
        atom_count: i32,
    ) -> StorageResult<WikiArticleWithCitations> {
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();

        // Archive existing article before replacing
        self.archive_existing_article(tag_id).await?;

        // Delete existing article for this tag (cascade deletes citations + links)
        sqlx::query("DELETE FROM wiki_articles WHERE tag_id = $1")
            .bind(tag_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        // Insert new article
        sqlx::query(
            "INSERT INTO wiki_articles (id, tag_id, content, created_at, updated_at, atom_count)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&id)
        .bind(tag_id)
        .bind(content)
        .bind(&now)
        .bind(&now)
        .bind(atom_count)
        .execute(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        // Insert citations
        for citation in citations {
            sqlx::query(
                "INSERT INTO wiki_citations (id, wiki_article_id, citation_index, atom_id, chunk_index, excerpt)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(&citation.id)
            .bind(&id)
            .bind(citation.citation_index)
            .bind(&citation.atom_id)
            .bind(citation.chunk_index)
            .bind(&citation.excerpt)
            .execute(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        }

        let article = WikiArticle {
            id,
            tag_id: tag_id.to_string(),
            content: content.to_string(),
            created_at: now.clone(),
            updated_at: now,
            atom_count,
        };

        Ok(WikiArticleWithCitations {
            article,
            citations: citations.to_vec(),
        })
    }

    async fn delete_wiki(&self, tag_id: &str) -> StorageResult<()> {
        sqlx::query("DELETE FROM wiki_articles WHERE tag_id = $1")
            .bind(tag_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        Ok(())
    }

    async fn get_wiki_links(&self, tag_id: &str) -> StorageResult<Vec<WikiLink>> {
        // The Postgres schema stores target_tag_id and link_text (not target_tag_name).
        // We adapt to produce WikiLink with target_tag_name resolved from tags table.
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT wl.id, wl.source_article_id, COALESCE(t.name, wl.link_text),
                    wl.target_tag_id
             FROM wiki_links wl
             LEFT JOIN tags t ON t.id = wl.target_tag_id
             WHERE wl.source_article_id = (SELECT id FROM wiki_articles WHERE tag_id = $1)",
        )
        .bind(tag_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let mut links = Vec::new();
        for (id, source_article_id, target_tag_name, target_tag_id) in rows {
            // Check if target tag has an article
            let has_article = if let Some(ref ttid) = target_tag_id {
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM wiki_articles WHERE tag_id = $1)",
                )
                .bind(ttid)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(false)
            } else {
                false
            };

            links.push(WikiLink {
                id,
                source_article_id,
                target_tag_name,
                target_tag_id,
                has_article,
            });
        }

        Ok(links)
    }

    async fn list_wiki_versions(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<WikiVersionSummary>> {
        let rows = sqlx::query_as::<_, (String, i32, i32, String)>(
            "SELECT id, version_number, atom_count, created_at
             FROM wiki_article_versions
             WHERE tag_id = $1
             ORDER BY version_number DESC",
        )
        .bind(tag_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(id, version_number, atom_count, created_at)| WikiVersionSummary {
                    id,
                    version_number,
                    atom_count,
                    created_at,
                },
            )
            .collect())
    }

    async fn get_wiki_version(
        &self,
        version_id: &str,
    ) -> StorageResult<Option<WikiArticleVersion>> {
        let row = sqlx::query_as::<_, (String, String, String, i32, i32, String)>(
            "SELECT id, tag_id, content, atom_count, version_number, created_at
             FROM wiki_article_versions WHERE id = $1",
        )
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        match row {
            Some((id, tag_id, content, atom_count, version_number, created_at)) => {
                // Postgres schema doesn't store citations_json in versions;
                // return empty citations for historical versions.
                Ok(Some(WikiArticleVersion {
                    id,
                    tag_id,
                    content,
                    citations: Vec::new(),
                    atom_count,
                    version_number,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_all_wiki_articles(&self) -> StorageResult<Vec<WikiArticleSummary>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, i32, i64)>(
            "SELECT w.id, w.tag_id, t.name, w.updated_at, w.atom_count,
                    (SELECT COUNT(*) FROM wiki_links wl WHERE wl.target_tag_id = w.tag_id)
             FROM wiki_articles w
             JOIN tags t ON w.tag_id = t.id
             ORDER BY (SELECT COUNT(*) FROM wiki_links wl WHERE wl.target_tag_id = w.tag_id) DESC,
                      w.atom_count DESC, w.updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(id, tag_id, tag_name, updated_at, atom_count, inbound_links)| {
                    WikiArticleSummary {
                        id,
                        tag_id,
                        tag_name,
                        updated_at,
                        atom_count,
                        inbound_links: inbound_links as i32,
                    }
                },
            )
            .collect())
    }

    async fn get_suggested_wiki_articles(
        &self,
        limit: i32,
    ) -> StorageResult<Vec<SuggestedArticle>> {
        // Postgres equivalent of the SQLite query, using SIMILAR TO instead of GLOB
        // and standard SQL features instead of SQLite-specific ones.
        let rows = sqlx::query_as::<_, (String, String, i32, i64, f64)>(
            "WITH link_mentions AS (
                SELECT tag_id, SUM(cnt) as link_count FROM (
                    SELECT wl.target_tag_id as tag_id, COUNT(*) as cnt
                    FROM wiki_links wl
                    WHERE wl.target_tag_id IS NOT NULL
                    GROUP BY wl.target_tag_id
                    UNION ALL
                    SELECT t2.id as tag_id, COUNT(*) as cnt
                    FROM wiki_links wl
                    JOIN tags t2 ON LOWER(wl.link_text) = LOWER(t2.name)
                    WHERE wl.target_tag_id IS NULL
                    GROUP BY t2.id
                ) sub
                GROUP BY tag_id
            )
            SELECT
                t.id,
                t.name,
                t.atom_count,
                COALESCE(lm.link_count, 0) as mention_count,
                t.atom_count * 1.0 + COALESCE(lm.link_count, 0) * 3.0 as score
            FROM tags t
            LEFT JOIN link_mentions lm ON lm.tag_id = t.id
            WHERE t.parent_id IS NOT NULL
              AND NOT EXISTS (SELECT 1 FROM wiki_articles wa WHERE wa.tag_id = t.id)
              AND t.name ~ '[^0-9]'
              AND LENGTH(t.name) >= 2
              AND t.atom_count > 0
            ORDER BY score DESC
            LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(tag_id, tag_name, atom_count, mention_count, score)| SuggestedArticle {
                    tag_id,
                    tag_name,
                    atom_count,
                    mention_count: mention_count as i32,
                    score,
                },
            )
            .collect())
    }
}

// Private helper methods
impl PostgresStorage {
    /// Archive the current wiki article (if any) into wiki_article_versions.
    async fn archive_existing_article(&self, tag_id: &str) -> StorageResult<()> {
        // Load existing article
        let existing = sqlx::query_as::<_, (String, String, i32, String)>(
            "SELECT id, content, atom_count, created_at FROM wiki_articles WHERE tag_id = $1",
        )
        .bind(tag_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let (_article_id, content, atom_count, created_at) = match existing {
            Some(e) => e,
            None => return Ok(()),
        };

        // Compute next version number
        let next_version: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
            "SELECT COALESCE(MAX(version_number), 0) + 1 FROM wiki_article_versions WHERE tag_id = $1",
        )
        .bind(tag_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        let next_version = next_version.unwrap_or(1);

        // Insert version (Postgres schema doesn't have citations_json column)
        sqlx::query(
            "INSERT INTO wiki_article_versions (id, tag_id, content, atom_count, version_number, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(tag_id)
        .bind(&content)
        .bind(atom_count)
        .bind(next_version as i32)
        .bind(&created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        Ok(())
    }
}
