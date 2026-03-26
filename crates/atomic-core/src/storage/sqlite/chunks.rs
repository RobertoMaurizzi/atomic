use std::collections::HashMap;

use super::SqliteStorage;
use crate::embedding;
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::storage::traits::*;
use async_trait::async_trait;
use uuid::Uuid;

impl SqliteStorage {
    pub(crate) fn get_pending_embeddings_sync(
        &self,
        limit: i32,
    ) -> StorageResult<Vec<(String, String)>> {
        let conn = self.db.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, content FROM atoms WHERE embedding_status = 'pending' LIMIT ?1",
        )?;
        let results = stmt
            .query_map([limit], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub(crate) fn set_embedding_status_sync(
        &self,
        atom_id: &str,
        status: &str,
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        conn.execute(
            "UPDATE atoms SET embedding_status = ?2 WHERE id = ?1",
            rusqlite::params![atom_id, status],
        )?;
        Ok(())
    }

    pub(crate) fn set_tagging_status_sync(
        &self,
        atom_id: &str,
        status: &str,
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        conn.execute(
            "UPDATE atoms SET tagging_status = ?2 WHERE id = ?1",
            rusqlite::params![atom_id, status],
        )?;
        Ok(())
    }

    pub(crate) fn save_chunks_and_embeddings_sync(
        &self,
        atom_id: &str,
        chunks: &[(String, Vec<f32>)],
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        // Remove old FTS entries before deleting chunks
        conn.execute(
            "INSERT INTO atom_chunks_fts(atom_chunks_fts, rowid, id, atom_id, chunk_index, content)
             SELECT 'delete', rowid, id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id = ?1",
            [atom_id],
        )
        .ok();

        // Delete existing vec_chunks
        conn.execute(
            "DELETE FROM vec_chunks WHERE chunk_id IN (SELECT id FROM atom_chunks WHERE atom_id = ?1)",
            [atom_id],
        )
        .ok();

        // Delete existing atom_chunks
        conn.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])?;

        // Insert new chunks and embeddings
        for (index, (chunk_content, embedding_vec)) in chunks.iter().enumerate() {
            let chunk_id = Uuid::new_v4().to_string();
            let embedding_blob = embedding::f32_vec_to_blob_public(embedding_vec);

            conn.execute(
                "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![&chunk_id, atom_id, index as i32, chunk_content, &embedding_blob],
            )?;

            conn.execute(
                "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
                rusqlite::params![&chunk_id, &embedding_blob],
            )?;
        }

        // Incrementally update FTS index
        conn.execute(
            "INSERT INTO atom_chunks_fts(rowid, id, atom_id, chunk_index, content)
             SELECT rowid, id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id = ?1",
            [atom_id],
        )
        .ok();

        Ok(())
    }

    pub(crate) fn delete_chunks_sync(&self, atom_id: &str) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        // Remove FTS entries
        conn.execute(
            "INSERT INTO atom_chunks_fts(atom_chunks_fts, rowid, id, atom_id, chunk_index, content)
             SELECT 'delete', rowid, id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id = ?1",
            [atom_id],
        )
        .ok();

        conn.execute(
            "DELETE FROM vec_chunks WHERE chunk_id IN (SELECT id FROM atom_chunks WHERE atom_id = ?1)",
            [atom_id],
        )
        .ok();

        conn.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])?;
        Ok(())
    }

    pub(crate) fn reset_stuck_processing_sync(&self) -> StorageResult<i32> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let embedding_count = conn.execute(
            "UPDATE atoms SET embedding_status = 'pending' WHERE embedding_status = 'processing'",
            [],
        )?;

        let tagging_count = conn.execute(
            "UPDATE atoms SET tagging_status = 'pending' WHERE tagging_status = 'processing'",
            [],
        )?;

        Ok((embedding_count + tagging_count) as i32)
    }

    pub(crate) fn rebuild_semantic_edges_sync(&self) -> StorageResult<i32> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let mut stmt = conn.prepare(
            "SELECT DISTINCT a.id FROM atoms a
             INNER JOIN atom_chunks ac ON a.id = ac.atom_id
             WHERE a.embedding_status = 'complete'",
        )?;

        let atom_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        conn.execute("DELETE FROM semantic_edges", [])?;

        let mut total_edges = 0;
        for (idx, atom_id) in atom_ids.iter().enumerate() {
            match embedding::compute_semantic_edges_for_atom(&conn, atom_id, 0.5, 15) {
                Ok(edge_count) => {
                    total_edges += edge_count;
                    if (idx + 1) % 50 == 0 {
                        eprintln!(
                            "Processed {}/{} atoms, {} edges so far",
                            idx + 1,
                            atom_ids.len(),
                            total_edges
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to compute edges for atom {}: {}",
                        atom_id, e
                    );
                }
            }
        }

        Ok(total_edges)
    }

    pub(crate) fn get_semantic_edges_sync(
        &self,
        min_similarity: f32,
    ) -> StorageResult<Vec<SemanticEdge>> {
        let conn = self.db.read_conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, source_atom_id, target_atom_id, similarity_score,
                    source_chunk_index, target_chunk_index, created_at
             FROM semantic_edges
             WHERE similarity_score >= ?1
             ORDER BY similarity_score DESC
             LIMIT 10000",
        )?;

        let edges = stmt
            .query_map([min_similarity], |row| {
                Ok(SemanticEdge {
                    id: row.get(0)?,
                    source_atom_id: row.get(1)?,
                    target_atom_id: row.get(2)?,
                    similarity_score: row.get(3)?,
                    source_chunk_index: row.get(4)?,
                    target_chunk_index: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(edges)
    }

    pub(crate) fn get_atom_neighborhood_sync(
        &self,
        atom_id: &str,
        depth: i32,
        min_similarity: f32,
    ) -> StorageResult<NeighborhoodGraph> {
        let conn = self.db.read_conn()?;
        crate::build_neighborhood_graph(&conn, atom_id, depth, min_similarity)
    }

    pub(crate) fn get_connection_counts_sync(
        &self,
        min_similarity: f32,
    ) -> StorageResult<HashMap<String, i32>> {
        let conn = self.db.read_conn()?;
        crate::clustering::get_connection_counts(&conn, min_similarity)
            .map_err(|e| AtomicCoreError::Clustering(e))
    }

    pub(crate) fn save_tag_centroid_sync(
        &self,
        tag_id: &str,
        embedding_vec: &[f32],
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let embedding_blob = embedding::f32_vec_to_blob_public(embedding_vec);
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO tag_embeddings (tag_id, embedding, atom_count, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![tag_id, &embedding_blob, 0, &now],
        )?;

        // vec0 doesn't support REPLACE, so delete + insert
        conn.execute("DELETE FROM vec_tags WHERE tag_id = ?1", [tag_id])
            .ok();
        conn.execute(
            "INSERT INTO vec_tags (tag_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![tag_id, &embedding_blob],
        )?;

        Ok(())
    }

    pub(crate) fn recompute_all_tag_embeddings_sync(&self) -> StorageResult<i32> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        // Get all tags that have at least one atom with embeddings
        let mut stmt = conn.prepare(
            "SELECT DISTINCT at.tag_id
             FROM atom_tags at
             INNER JOIN atom_chunks ac ON at.atom_id = ac.atom_id
             WHERE ac.embedding IS NOT NULL",
        )?;

        let tag_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let count = tag_ids.len() as i32;
        eprintln!("Recomputing centroid embeddings for {} tags...", count);

        embedding::compute_tag_embeddings_batch(&conn, &tag_ids)
            .map_err(|e| AtomicCoreError::Embedding(e))?;

        eprintln!("Tag centroid embeddings recomputed for {} tags", count);
        Ok(count)
    }

    pub(crate) fn check_vector_extension_sync(&self) -> StorageResult<String> {
        let conn = self.db.read_conn()?;
        let version: String =
            conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
        Ok(version)
    }
}

#[async_trait]
impl ChunkStore for SqliteStorage {
    async fn get_pending_embeddings(&self, limit: i32) -> StorageResult<Vec<(String, String)>> {
        self.get_pending_embeddings_sync(limit)
    }

    async fn set_embedding_status(
        &self,
        atom_id: &str,
        status: &str,
    ) -> StorageResult<()> {
        self.set_embedding_status_sync(atom_id, status)
    }

    async fn set_tagging_status(
        &self,
        atom_id: &str,
        status: &str,
    ) -> StorageResult<()> {
        self.set_tagging_status_sync(atom_id, status)
    }

    async fn save_chunks_and_embeddings(
        &self,
        atom_id: &str,
        chunks: &[(String, Vec<f32>)],
    ) -> StorageResult<()> {
        self.save_chunks_and_embeddings_sync(atom_id, chunks)
    }

    async fn delete_chunks(&self, atom_id: &str) -> StorageResult<()> {
        self.delete_chunks_sync(atom_id)
    }

    async fn reset_stuck_processing(&self) -> StorageResult<i32> {
        self.reset_stuck_processing_sync()
    }

    async fn rebuild_semantic_edges(&self) -> StorageResult<i32> {
        self.rebuild_semantic_edges_sync()
    }

    async fn get_semantic_edges(
        &self,
        min_similarity: f32,
    ) -> StorageResult<Vec<SemanticEdge>> {
        self.get_semantic_edges_sync(min_similarity)
    }

    async fn get_atom_neighborhood(
        &self,
        atom_id: &str,
        depth: i32,
        min_similarity: f32,
    ) -> StorageResult<NeighborhoodGraph> {
        self.get_atom_neighborhood_sync(atom_id, depth, min_similarity)
    }

    async fn get_connection_counts(
        &self,
        min_similarity: f32,
    ) -> StorageResult<HashMap<String, i32>> {
        self.get_connection_counts_sync(min_similarity)
    }

    async fn save_tag_centroid(
        &self,
        tag_id: &str,
        embedding: &[f32],
    ) -> StorageResult<()> {
        self.save_tag_centroid_sync(tag_id, embedding)
    }

    async fn recompute_all_tag_embeddings(&self) -> StorageResult<i32> {
        self.recompute_all_tag_embeddings_sync()
    }

    async fn check_vector_extension(&self) -> StorageResult<String> {
        self.check_vector_extension_sync()
    }
}
