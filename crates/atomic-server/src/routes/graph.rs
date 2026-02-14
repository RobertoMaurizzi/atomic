//! Semantic graph routes

use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct EdgesQuery {
    pub min_similarity: Option<f32>,
}

pub async fn get_semantic_edges(
    state: web::Data<AppState>,
    query: web::Query<EdgesQuery>,
) -> HttpResponse {
    let min_similarity = query.min_similarity.unwrap_or(0.5);
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    let result: Result<Vec<atomic_core::SemanticEdge>, rusqlite::Error> = (|| {
        let mut stmt = conn.prepare(
            "SELECT id, source_atom_id, target_atom_id, similarity_score,
                    source_chunk_index, target_chunk_index, created_at
             FROM semantic_edges
             WHERE similarity_score >= ?1
             ORDER BY similarity_score DESC",
        )?;
        let edges = stmt
            .query_map([min_similarity], |row| {
                Ok(atomic_core::SemanticEdge {
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
    })();

    match result {
        Ok(edges) => HttpResponse::Ok().json(edges),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": format!("{}", e)})),
    }
}

#[derive(Deserialize)]
pub struct NeighborhoodQuery {
    pub depth: Option<i32>,
    pub min_similarity: Option<f32>,
}

pub async fn get_atom_neighborhood(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<NeighborhoodQuery>,
) -> HttpResponse {
    let atom_id = path.into_inner();
    let depth = query.depth.unwrap_or(1);
    let min_similarity = query.min_similarity.unwrap_or(0.5);

    // This is complex — delegate to a helper that mirrors the Tauri command logic
    // For now, use the same DB-level approach
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    match build_neighborhood(&conn, &atom_id, depth, min_similarity) {
        Ok(graph) => HttpResponse::Ok().json(graph),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e})),
    }
}

pub async fn rebuild_semantic_edges(state: web::Data<AppState>) -> HttpResponse {
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    // Get all atoms with complete embeddings
    let atom_ids: Vec<String> = match conn
        .prepare(
            "SELECT DISTINCT a.id FROM atoms a
             INNER JOIN atom_chunks ac ON a.id = ac.atom_id
             WHERE a.embedding_status = 'complete'",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get(0))
                .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        }) {
        Ok(ids) => ids,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    // Clear existing edges
    if let Err(e) = conn.execute("DELETE FROM semantic_edges", []) {
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e.to_string()}));
    }

    let mut total_edges = 0;
    for atom_id in &atom_ids {
        match atomic_core::embedding::compute_semantic_edges_for_atom(&conn, atom_id, 0.5, 15) {
            Ok(count) => total_edges += count,
            Err(e) => eprintln!("Warning: Failed to compute edges for {}: {}", atom_id, e),
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "atoms_processed": atom_ids.len(),
        "total_edges": total_edges
    }))
}

/// Build neighborhood graph (mirrors src-tauri/src/commands/graph.rs logic)
fn build_neighborhood(
    conn: &rusqlite::Connection,
    atom_id: &str,
    depth: i32,
    min_similarity: f32,
) -> Result<atomic_core::NeighborhoodGraph, String> {
    use std::collections::HashMap;

    let mut atoms_at_depth: HashMap<String, i32> = HashMap::new();
    atoms_at_depth.insert(atom_id.to_string(), 0);

    // Depth 1 semantic connections
    {
        let mut stmt = conn
            .prepare(
                "SELECT
                    CASE WHEN source_atom_id = ?1 THEN target_atom_id ELSE source_atom_id END as other_atom_id,
                    similarity_score
                 FROM semantic_edges
                 WHERE (source_atom_id = ?1 OR target_atom_id = ?1)
                   AND similarity_score >= ?2
                 ORDER BY similarity_score DESC
                 LIMIT 20",
            )
            .map_err(|e| e.to_string())?;

        let results: Vec<(String, f32)> = stmt
            .query_map(rusqlite::params![atom_id, min_similarity], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        for (other_id, _) in &results {
            atoms_at_depth.entry(other_id.clone()).or_insert(1);
        }
    }

    // Depth 1 tag connections
    let center_tags: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT tag_id FROM atom_tags WHERE atom_id = ?1")
            .map_err(|e| e.to_string())?;
        let r = stmt
            .query_map([atom_id], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        r
    };

    if !center_tags.is_empty() {
        let placeholders: String = center_tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT atom_id, COUNT(*) as shared_count
             FROM atom_tags
             WHERE tag_id IN ({})
               AND atom_id != ?
             GROUP BY atom_id
             HAVING shared_count >= 1
             ORDER BY shared_count DESC
             LIMIT 20",
            placeholders
        );
        let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
        let mut params: Vec<&dyn rusqlite::ToSql> = center_tags
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();
        params.push(&atom_id);

        let tag_results: Vec<(String, i32)> = stmt
            .query_map(params.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        for (other_id, _) in &tag_results {
            atoms_at_depth.entry(other_id.clone()).or_insert(1);
        }
    }

    // Depth 2 if requested
    if depth >= 2 {
        let depth1_ids: Vec<String> = atoms_at_depth
            .iter()
            .filter(|(_, d)| **d == 1)
            .map(|(id, _)| id.clone())
            .collect();

        for d1_id in &depth1_ids {
            let mut stmt = conn
                .prepare(
                    "SELECT
                        CASE WHEN source_atom_id = ?1 THEN target_atom_id ELSE source_atom_id END
                     FROM semantic_edges
                     WHERE (source_atom_id = ?1 OR target_atom_id = ?1)
                       AND similarity_score >= ?2
                     ORDER BY similarity_score DESC
                     LIMIT 5",
                )
                .map_err(|e| e.to_string())?;

            let d2_ids: Vec<String> = stmt
                .query_map(rusqlite::params![d1_id, min_similarity], |row| row.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            for d2_id in d2_ids {
                atoms_at_depth.entry(d2_id).or_insert(2);
            }
        }
    }

    // Limit total atoms
    let max_atoms = if depth >= 2 { 30 } else { 20 };
    let mut sorted_atoms: Vec<(String, i32)> = atoms_at_depth.into_iter().collect();
    sorted_atoms.sort_by_key(|(_, d)| *d);
    sorted_atoms.truncate(max_atoms);

    let atom_ids: Vec<String> = sorted_atoms.iter().map(|(id, _)| id.clone()).collect();
    let atom_depths: HashMap<String, i32> = sorted_atoms.into_iter().collect();

    // Fetch atom data
    let mut atoms = Vec::new();
    for aid in &atom_ids {
        let atom = conn
            .query_row(
                "SELECT id, content, source_url, created_at, updated_at,
                        COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending')
                 FROM atoms WHERE id = ?1",
                [aid],
                |row| {
                    Ok(atomic_core::Atom {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        source_url: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        embedding_status: row.get(5)?,
                        tagging_status: row.get(6)?,
                    })
                },
            )
            .map_err(|e| format!("Failed to get atom {}: {}", aid, e))?;

        let tags = get_tags_for_atom(conn, aid)?;
        let depth = *atom_depths.get(aid).unwrap_or(&0);

        atoms.push(atomic_core::NeighborhoodAtom {
            atom: atomic_core::AtomWithTags { atom, tags },
            depth,
        });
    }

    // Build edges
    let mut edges = Vec::new();
    for i in 0..atom_ids.len() {
        for j in (i + 1)..atom_ids.len() {
            let id_a = &atom_ids[i];
            let id_b = &atom_ids[j];

            let semantic_score: Option<f32> = conn
                .query_row(
                    "SELECT similarity_score FROM semantic_edges
                     WHERE (source_atom_id = ?1 AND target_atom_id = ?2)
                        OR (source_atom_id = ?2 AND target_atom_id = ?1)",
                    [id_a, id_b],
                    |row| row.get(0),
                )
                .ok();

            let shared_tags: i32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM atom_tags a1
                     INNER JOIN atom_tags a2 ON a1.tag_id = a2.tag_id
                     WHERE a1.atom_id = ?1 AND a2.atom_id = ?2",
                    [id_a, id_b],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            if semantic_score.is_some() || shared_tags > 0 {
                let edge_type = match (semantic_score.is_some(), shared_tags > 0) {
                    (true, true) => "both",
                    (true, false) => "semantic",
                    (false, true) => "tag",
                    (false, false) => continue,
                };

                let semantic_strength = semantic_score.unwrap_or(0.0);
                let tag_strength = (shared_tags as f32 * 0.15).min(0.6);
                let strength = (semantic_strength + tag_strength).min(1.0);

                edges.push(atomic_core::NeighborhoodEdge {
                    source_id: id_a.clone(),
                    target_id: id_b.clone(),
                    edge_type: edge_type.to_string(),
                    strength,
                    shared_tag_count: shared_tags,
                    similarity_score: semantic_score,
                });
            }
        }
    }

    Ok(atomic_core::NeighborhoodGraph {
        center_atom_id: atom_id.to_string(),
        atoms,
        edges,
    })
}

fn get_tags_for_atom(
    conn: &rusqlite::Connection,
    atom_id: &str,
) -> Result<Vec<atomic_core::Tag>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.parent_id, t.created_at
             FROM tags t
             INNER JOIN atom_tags at ON t.id = at.tag_id
             WHERE at.atom_id = ?1",
        )
        .map_err(|e| e.to_string())?;

    let tags = stmt
        .query_map([atom_id], |row| {
            Ok(atomic_core::Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(tags)
}
