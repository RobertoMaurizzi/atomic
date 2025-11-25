use crate::chunking::chunk_content;
use crate::db::Database;
use crate::models::EmbeddingCompletePayload;
use std::sync::Arc;
use tauri::AppHandle;
use tauri::Emitter;
use uuid::Uuid;

/// Process embeddings for an atom asynchronously
/// This spawns a background task that:
/// 1. Sets embedding_status to 'processing'
/// 2. Chunks the content
/// 3. Generates real embeddings for each chunk using sqlite-lembed
/// 4. Stores chunks and embeddings in database
/// 5. Sets embedding_status to 'complete' or 'failed'
/// 6. Emits 'embedding-complete' event
pub fn spawn_embedding_task(
    app_handle: AppHandle,
    db: Arc<Database>,
    atom_id: String,
    content: String,
) {
    std::thread::spawn(move || {
        let result = process_embeddings(&db, &atom_id, &content);

        let payload = match result {
            Ok(()) => EmbeddingCompletePayload {
                atom_id: atom_id.clone(),
                status: "complete".to_string(),
                error: None,
            },
            Err(e) => {
                // Update status to failed
                if let Ok(conn) = db.conn.lock() {
                    let _ = conn.execute(
                        "UPDATE atoms SET embedding_status = 'failed' WHERE id = ?1",
                        [&atom_id],
                    );
                }
                EmbeddingCompletePayload {
                    atom_id: atom_id.clone(),
                    status: "failed".to_string(),
                    error: Some(e),
                }
            }
        };

        // Emit event to frontend
        let _ = app_handle.emit("embedding-complete", payload);
    });
}

fn process_embeddings(db: &Database, atom_id: &str, content: &str) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Set status to processing
    conn.execute(
        "UPDATE atoms SET embedding_status = 'processing' WHERE id = ?1",
        [atom_id],
    )
    .map_err(|e| e.to_string())?;

    // First, get existing chunk IDs for this atom to delete from vec_chunks
    let existing_chunk_ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM atom_chunks WHERE atom_id = ?1")
            .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;
        let ids = stmt
            .query_map([atom_id], |row| row.get(0))
            .map_err(|e| format!("Failed to query chunks: {}", e))?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|e| format!("Failed to collect chunk IDs: {}", e))?;
        ids
    };

    // Delete existing vec_chunks entries for this atom's chunks
    for chunk_id in &existing_chunk_ids {
        conn.execute("DELETE FROM vec_chunks WHERE chunk_id = ?1", [chunk_id])
            .ok(); // Ignore errors if chunk doesn't exist in vec_chunks
    }

    // Delete existing chunks for this atom
    conn.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])
        .map_err(|e| e.to_string())?;

    // Chunk the content
    let chunks = chunk_content(content);

    if chunks.is_empty() {
        // No chunks to process, mark as complete
        conn.execute(
            "UPDATE atoms SET embedding_status = 'complete' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Process each chunk with REAL embeddings using sqlite-lembed
    for (index, chunk_content) in chunks.iter().enumerate() {
        let chunk_id = Uuid::new_v4().to_string();

        // Generate REAL embedding using sqlite-lembed
        let embedding_blob: Vec<u8> = conn
            .query_row(
                "SELECT lembed('all-MiniLM-L6-v2', ?1)",
                [chunk_content],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to generate embedding: {}", e))?;

        // Insert into atom_chunks
        conn.execute(
            "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![&chunk_id, atom_id, index as i32, chunk_content, &embedding_blob],
        )
        .map_err(|e| format!("Failed to insert chunk: {}", e))?;

        // Insert into vec_chunks for similarity search
        conn.execute(
            "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![&chunk_id, &embedding_blob],
        )
        .map_err(|e| format!("Failed to insert vec_chunk: {}", e))?;
    }

    // Set status to complete
    conn.execute(
        "UPDATE atoms SET embedding_status = 'complete' WHERE id = ?1",
        [atom_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Convert distance to similarity score (0-1 scale)
/// For normalized vectors using L2 distance, max distance is 2.0 (opposite vectors)
pub fn distance_to_similarity(distance: f32) -> f32 {
    // For L2 distance on normalized vectors:
    // distance = 0 means identical vectors (similarity = 1)
    // distance = 2 means opposite vectors (similarity = 0)
    (1.0 - (distance / 2.0)).max(0.0).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_to_similarity() {
        assert!((distance_to_similarity(0.0) - 1.0).abs() < 0.001);
        assert!((distance_to_similarity(2.0) - 0.0).abs() < 0.001);
        assert!((distance_to_similarity(1.0) - 0.5).abs() < 0.001);
    }
}

