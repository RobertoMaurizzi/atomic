use super::PostgresStorage;
use crate::compaction::{CompactionResult, TagMerge};
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::storage::traits::*;
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;
use chrono::Utc;

impl PostgresStorage {
    /// Load all tags and their direct (denormalized) atom counts.
    async fn load_tags_and_counts(&self) -> StorageResult<(Vec<Tag>, HashMap<String, i32>)> {
        let rows: Vec<(String, String, Option<String>, String, i32)> = sqlx::query_as(
            "SELECT id, name, parent_id, created_at, atom_count FROM tags ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let mut direct_counts: HashMap<String, i32> = HashMap::new();
        let all_tags: Vec<Tag> = rows
            .into_iter()
            .map(|(id, name, parent_id, created_at, count)| {
                direct_counts.insert(id.clone(), count);
                Tag {
                    id,
                    name,
                    parent_id,
                    created_at,
                }
            })
            .collect();

        Ok((all_tags, direct_counts))
    }

    /// Check if a tag is a descendant of another tag (for merge safety).
    async fn is_descendant_of(
        &self,
        potential_child: &str,
        potential_parent: &str,
    ) -> StorageResult<bool> {
        let mut current = potential_child.to_string();
        let mut visited = std::collections::HashSet::new();

        loop {
            if current == potential_parent {
                return Ok(true);
            }
            if visited.contains(&current) {
                return Ok(false);
            }
            visited.insert(current.clone());

            let parent: Option<String> = sqlx::query_scalar(
                "SELECT parent_id FROM tags WHERE id = $1",
            )
            .bind(&current)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?
            .flatten();

            match parent {
                Some(p) => current = p,
                None => return Ok(false),
            }
        }
    }

    /// Look up a tag ID by its name (case-insensitive).
    async fn get_tag_id_by_name(&self, name: &str) -> StorageResult<Option<String>> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM tags WHERE LOWER(name) = LOWER($1)",
        )
        .bind(name.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        Ok(id)
    }

    /// Execute a single tag merge: move atoms from loser to winner, reparent children, delete loser.
    async fn execute_tag_merge(
        &self,
        merge: &TagMerge,
    ) -> Result<(bool, i32), String> {
        let winner_id = match self.get_tag_id_by_name(&merge.winner_name).await
            .map_err(|e| e.to_string())?
        {
            Some(id) => id,
            None => {
                eprintln!("Skipping merge: winner '{}' not found", merge.winner_name);
                return Ok((false, 0));
            }
        };

        let loser_id = match self.get_tag_id_by_name(&merge.loser_name).await
            .map_err(|e| e.to_string())?
        {
            Some(id) => id,
            None => {
                eprintln!("Skipping merge: loser '{}' not found", merge.loser_name);
                return Ok((false, 0));
            }
        };

        if winner_id == loser_id {
            eprintln!(
                "Skipping merge: '{}' and '{}' are the same tag",
                merge.winner_name, merge.loser_name
            );
            return Ok((false, 0));
        }

        if self.is_descendant_of(&loser_id, &winner_id).await
            .map_err(|e| e.to_string())?
        {
            eprintln!(
                "Skipping merge: '{}' is a descendant of '{}'",
                merge.loser_name, merge.winner_name
            );
            return Ok((false, 0));
        }
        if self.is_descendant_of(&winner_id, &loser_id).await
            .map_err(|e| e.to_string())?
        {
            eprintln!(
                "Skipping merge: '{}' is a descendant of '{}'",
                merge.winner_name, merge.loser_name
            );
            return Ok((false, 0));
        }

        // Get atoms tagged with the loser
        let atoms_with_loser: Vec<String> = sqlx::query_scalar(
            "SELECT atom_id FROM atom_tags WHERE tag_id = $1",
        )
        .bind(&loser_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to query atoms: {}", e))?;

        let mut atoms_retagged: i32 = 0;
        for atom_id in &atoms_with_loser {
            // INSERT ... ON CONFLICT DO NOTHING replaces INSERT OR IGNORE
            let result = sqlx::query(
                "INSERT INTO atom_tags (atom_id, tag_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(atom_id)
            .bind(&winner_id)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("Failed to add winner tag: {}", e))?;

            if result.rows_affected() > 0 {
                atoms_retagged += 1;
            }
        }

        // Reparent children of the loser to the winner
        sqlx::query("UPDATE tags SET parent_id = $1 WHERE parent_id = $2")
            .bind(&winner_id)
            .bind(&loser_id)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("Failed to reparent children: {}", e))?;

        // Delete the loser tag (atom_tags rows will be cleaned by cascade or explicit delete)
        sqlx::query("DELETE FROM atom_tags WHERE tag_id = $1")
            .bind(&loser_id)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("Failed to delete loser atom_tags: {}", e))?;

        sqlx::query("DELETE FROM tags WHERE id = $1")
            .bind(&loser_id)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("Failed to delete loser tag: {}", e))?;

        eprintln!(
            "Merged '{}' into '{}' ({} atoms retagged): {}",
            merge.loser_name, merge.winner_name, atoms_retagged, merge.reason
        );

        Ok((true, atoms_retagged))
    }
}

#[async_trait]
impl TagStore for PostgresStorage {
    async fn get_all_tags(&self) -> StorageResult<Vec<TagWithCount>> {
        self.get_all_tags_filtered(0).await
    }

    async fn get_all_tags_filtered(&self, min_count: i32) -> StorageResult<Vec<TagWithCount>> {
        let (all_tags, direct_counts) = self.load_tags_and_counts().await?;
        Ok(crate::build_tag_tree_with_counts(
            &all_tags,
            None,
            &direct_counts,
            min_count,
        ))
    }

    async fn get_tag_children(
        &self,
        parent_id: &str,
        min_count: i32,
        limit: i32,
        offset: i32,
    ) -> StorageResult<PaginatedTagChildren> {
        // Fast total count
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tags WHERE parent_id = $1",
        )
        .bind(parent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let total = total as i32;

        if total == 0 {
            return Ok(PaginatedTagChildren {
                children: Vec::new(),
                total: 0,
            });
        }

        let rows: Vec<(String, String, Option<String>, String, i32, i64)> = sqlx::query_as(
            "SELECT t.id, t.name, t.parent_id, t.created_at, t.atom_count,
                (SELECT COUNT(*) FROM tags c WHERE c.parent_id = t.id) AS children_total
            FROM tags t
            WHERE t.parent_id = $1
            ORDER BY t.atom_count DESC
            LIMIT $2 OFFSET $3",
        )
        .bind(parent_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let mut children: Vec<TagWithCount> = rows
            .into_iter()
            .map(|(id, name, parent_id, created_at, atom_count, children_total)| TagWithCount {
                tag: Tag {
                    id,
                    name,
                    parent_id,
                    created_at,
                },
                atom_count,
                children_total: children_total as i32,
                children: Vec::new(),
            })
            .collect();

        if min_count > 0 {
            children.retain(|t| t.atom_count >= min_count || t.children_total > 0);
        }

        Ok(PaginatedTagChildren { children, total })
    }

    async fn create_tag(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO tags (id, name, parent_id, created_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(&id)
        .bind(name)
        .bind(parent_id)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        Ok(Tag {
            id,
            name: name.to_string(),
            parent_id: parent_id.map(String::from),
            created_at: now,
        })
    }

    async fn update_tag(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag> {
        sqlx::query("UPDATE tags SET name = $1, parent_id = $2 WHERE id = $3")
            .bind(name)
            .bind(parent_id)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let row: (String, String, Option<String>, String) = sqlx::query_as(
            "SELECT id, name, parent_id, created_at FROM tags WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        Ok(Tag {
            id: row.0,
            name: row.1,
            parent_id: row.2,
            created_at: row.3,
        })
    }

    async fn delete_tag(&self, id: &str, recursive: bool) -> StorageResult<()> {
        if recursive {
            // Delete tag and all descendants via recursive CTE.
            // In Postgres, we use a CTE with DELETE.
            sqlx::query(
                "WITH RECURSIVE descendants(id) AS (
                    SELECT id FROM tags WHERE id = $1
                    UNION ALL
                    SELECT t.id FROM tags t JOIN descendants d ON t.parent_id = d.id
                )
                DELETE FROM tags WHERE id IN (SELECT id FROM descendants)",
            )
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        } else {
            sqlx::query("DELETE FROM tags WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        }

        Ok(())
    }

    async fn get_related_tags(
        &self,
        tag_id: &str,
        limit: usize,
    ) -> StorageResult<Vec<RelatedTag>> {
        // Get tag hierarchy (this tag + all descendants) for exclusion
        let source_tag_ids: Vec<String> = sqlx::query_scalar(
            "WITH RECURSIVE descendant_tags(id) AS (
                SELECT $1::text
                UNION ALL
                SELECT t.id FROM tags t
                INNER JOIN descendant_tags dt ON t.parent_id = dt.id
            )
            SELECT id FROM descendant_tags",
        )
        .bind(tag_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        if source_tag_ids.is_empty() {
            return Ok(Vec::new());
        }

        let exclude_set: std::collections::HashSet<&str> =
            source_tag_ids.iter().map(|s| s.as_str()).collect();

        let mut tags: Vec<RelatedTag> = Vec::new();
        let mut tag_map: HashMap<String, usize> = HashMap::new();

        // === Signal 1: Shared atoms (co-occurrence) ===
        {
            let shared_limit = (limit * 3).max(30) as i32;
            let rows: Vec<(String, String, i64, i32)> = sqlx::query_as(
                "SELECT t.id, t.name, COUNT(DISTINCT at1.atom_id) as shared_count,
                        CASE WHEN wa.id IS NOT NULL THEN 1 ELSE 0 END as has_article
                 FROM atom_tags at1
                 JOIN atom_tags at2 ON at1.atom_id = at2.atom_id
                 JOIN tags t ON at2.tag_id = t.id
                 LEFT JOIN wiki_articles wa ON t.id = wa.tag_id
                 WHERE at1.tag_id IN (SELECT id FROM tags WHERE id = $1 OR parent_id = $1)
                   AND at2.tag_id NOT IN (SELECT id FROM tags WHERE id = $1 OR parent_id = $1)
                   AND t.parent_id IS NOT NULL
                 GROUP BY t.id, t.name, wa.id
                 ORDER BY shared_count DESC
                 LIMIT $2",
            )
            .bind(tag_id)
            .bind(shared_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

            for (tid, tname, shared_count, has_article_int) in rows {
                let shared_atoms = shared_count as i32;
                let rt = RelatedTag {
                    tag_id: tid.clone(),
                    tag_name: tname,
                    score: (shared_atoms as f64) * 0.4,
                    shared_atoms,
                    semantic_edges: 0,
                    has_article: has_article_int == 1,
                };
                tag_map.insert(tid, tags.len());
                tags.push(rt);
            }
        }

        // === Signal 2: Tag centroid embedding similarity ===
        // In Postgres with pgvector, we query the tag_embeddings table using <-> operator.
        let source_embedding: Option<Vec<f32>> = sqlx::query_scalar(
            "SELECT embedding::real[] FROM tag_embeddings WHERE tag_id = $1",
        )
        .bind(tag_id)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or(None);

        if let Some(ref _source_emb) = source_embedding {
            let centroid_limit = (limit * 3).max(30) as i64;
            // Use pgvector's <-> (L2 distance) operator for nearest-neighbor search
            let centroid_rows: Vec<(String, f64)> = sqlx::query_as(
                "SELECT te.tag_id, te.embedding <-> (SELECT embedding FROM tag_embeddings WHERE tag_id = $1) as distance
                 FROM tag_embeddings te
                 WHERE te.tag_id != $1
                 ORDER BY distance
                 LIMIT $2",
            )
            .bind(tag_id)
            .bind(centroid_limit)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

            let mut new_candidates: Vec<(String, f64)> = Vec::new();
            for (candidate_tag_id, distance) in &centroid_rows {
                if exclude_set.contains(candidate_tag_id.as_str()) {
                    continue;
                }
                // Convert L2 distance to similarity: 1 - (d^2 / 2) for normalized vectors
                let centroid_sim = 1.0 - (distance * distance / 2.0);
                if centroid_sim < 0.3 {
                    continue;
                }
                let centroid_score = centroid_sim * 0.6;

                if let Some(&idx) = tag_map.get(candidate_tag_id) {
                    tags[idx].score += centroid_score;
                } else {
                    new_candidates.push((candidate_tag_id.clone(), centroid_score));
                }
            }

            // Batch lookup metadata for new centroid-only candidates
            if !new_candidates.is_empty() {
                let placeholders: Vec<String> = (1..=new_candidates.len())
                    .map(|i| format!("${}", i))
                    .collect();
                let query = format!(
                    "SELECT t.id, t.name, CASE WHEN wa.id IS NOT NULL THEN 1 ELSE 0 END
                     FROM tags t
                     LEFT JOIN wiki_articles wa ON t.id = wa.tag_id
                     WHERE t.id IN ({}) AND t.parent_id IS NOT NULL",
                    placeholders.join(", ")
                );

                let mut q = sqlx::query_as::<_, (String, String, i32)>(&query);
                for (cid, _) in &new_candidates {
                    q = q.bind(cid);
                }

                let meta_rows = q
                    .fetch_all(&self.pool)
                    .await
                    .unwrap_or_default();

                let score_map: HashMap<&str, f64> = new_candidates
                    .iter()
                    .map(|(id, score)| (id.as_str(), *score))
                    .collect();

                for (id, name, has_article_int) in meta_rows {
                    let centroid_score = score_map.get(id.as_str()).copied().unwrap_or(0.0);
                    tag_map.insert(id.clone(), tags.len());
                    tags.push(RelatedTag {
                        tag_id: id,
                        tag_name: name,
                        score: centroid_score,
                        shared_atoms: 0,
                        semantic_edges: 0,
                        has_article: has_article_int == 1,
                    });
                }
            }
        }

        // === Signal 3: Content mentions ===
        // Tags whose names appear in this tag's wiki article content.
        let article_content: Option<String> = sqlx::query_scalar(
            "SELECT content FROM wiki_articles WHERE tag_id = $1",
        )
        .bind(tag_id)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or(None);

        if let Some(content) = article_content {
            let content_lower = content.to_lowercase();

            // Build exclusion placeholders
            let placeholders: Vec<String> = (1..=source_tag_ids.len())
                .map(|i| format!("${}", i))
                .collect();
            let mention_query = format!(
                "SELECT t.id, t.name,
                        CASE WHEN wa.id IS NOT NULL THEN 1 ELSE 0 END as has_article
                 FROM tags t
                 LEFT JOIN wiki_articles wa ON t.id = wa.tag_id
                 WHERE t.parent_id IS NOT NULL
                   AND t.id NOT IN ({})
                   AND length(t.name) >= 3
                   AND t.name ~ '[^0-9]'",
                placeholders.join(", ")
            );

            let mut q = sqlx::query_as::<_, (String, String, i32)>(&mention_query);
            for tid in &source_tag_ids {
                q = q.bind(tid);
            }

            let candidate_tags = q
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();

            // Filter by whole-word name match in content
            let matched_tags: Vec<(String, String, bool)> = candidate_tags
                .into_iter()
                .filter(|(_, name, _)| {
                    let name_lower = name.to_lowercase();
                    if let Some(pos) = content_lower.find(&name_lower) {
                        let before_ok = pos == 0
                            || !content_lower.as_bytes()[pos - 1].is_ascii_alphanumeric();
                        let end = pos + name_lower.len();
                        let after_ok = end >= content_lower.len()
                            || !content_lower.as_bytes()[end].is_ascii_alphanumeric();
                        before_ok && after_ok
                    } else {
                        false
                    }
                })
                .map(|(id, name, ha)| (id, name, ha == 1))
                .collect();

            for (tid, tname, has_article) in matched_tags {
                if !tag_map.contains_key(&tid) {
                    tag_map.insert(tid.clone(), tags.len());
                    tags.push(RelatedTag {
                        tag_id: tid,
                        tag_name: tname,
                        score: 0.1, // small boost for content mention
                        shared_atoms: 0,
                        semantic_edges: 0,
                        has_article,
                    });
                }
            }
        }

        // Sort by score and truncate
        tags.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        tags.truncate(limit);

        Ok(tags)
    }

    async fn get_tags_for_compaction(&self) -> StorageResult<String> {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT t.name, p.name as parent_name
             FROM tags t
             LEFT JOIN tags p ON t.parent_id = p.id
             ORDER BY COALESCE(p.name, t.name), t.name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        if rows.is_empty() {
            return Ok("(no existing tags)".to_string());
        }

        let mut result = String::new();
        let mut current_parent: Option<String> = None;

        for (name, parent) in rows {
            match (&parent, &current_parent) {
                (Some(p), Some(cp)) if p == cp => {
                    result.push_str(&format!("  - {}\n", name));
                }
                (Some(p), _) => {
                    result.push_str(&format!("{}\n", p));
                    result.push_str(&format!("  - {}\n", name));
                    current_parent = Some(p.clone());
                }
                (None, _) => {
                    result.push_str(&format!("{}\n", name));
                    current_parent = None;
                }
            }
        }

        Ok(result.trim_end().to_string())
    }

    async fn apply_tag_merges(
        &self,
        merges: &[TagMerge],
    ) -> StorageResult<CompactionResult> {
        let mut tags_merged = 0;
        let mut atoms_retagged = 0;
        let mut errors = Vec::new();

        for merge in merges {
            match self.execute_tag_merge(merge).await {
                Ok((true, retagged)) => {
                    tags_merged += 1;
                    atoms_retagged += retagged;
                }
                Ok((false, _)) => {}
                Err(e) => errors.push(format!(
                    "Error merging '{}' -> '{}': {}",
                    merge.loser_name, merge.winner_name, e
                )),
            }
        }

        if !errors.is_empty() {
            eprintln!("Merge errors: {:?}", errors);
        }

        Ok(CompactionResult {
            tags_merged,
            atoms_retagged,
        })
    }
}
