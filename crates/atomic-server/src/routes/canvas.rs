//! Canvas position routes

use crate::state::AppState;
use actix_web::{web, HttpResponse};
use atomic_core::AtomPosition;

pub async fn get_positions(state: web::Data<AppState>) -> HttpResponse {
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    let result: Result<Vec<AtomPosition>, rusqlite::Error> = (|| {
        let mut stmt = conn.prepare("SELECT atom_id, x, y FROM atom_positions")?;
        let positions = stmt
            .query_map([], |row| {
                Ok(AtomPosition {
                    atom_id: row.get(0)?,
                    x: row.get(1)?,
                    y: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(positions)
    })();

    match result {
        Ok(positions) => HttpResponse::Ok().json(positions),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": format!("{}", e)})),
    }
}

pub async fn save_positions(
    state: web::Data<AppState>,
    body: web::Json<Vec<AtomPosition>>,
) -> HttpResponse {
    let positions = body.into_inner();
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    for pos in &positions {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO atom_positions (atom_id, x, y, updated_at) VALUES (?1, ?2, ?3, ?4)",
            (&pos.atom_id, &pos.x, &pos.y, &now),
        ) {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    }

    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

pub async fn get_atoms_with_embeddings(state: web::Data<AppState>) -> HttpResponse {
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    // Get all atoms
    let atoms_result: Result<Vec<atomic_core::Atom>, rusqlite::Error> = (|| {
        let mut stmt = conn.prepare(
            "SELECT id, content, source_url, created_at, updated_at,
             COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending')
             FROM atoms ORDER BY updated_at DESC",
        )?;
        let atoms = stmt
            .query_map([], |row| {
                Ok(atomic_core::Atom {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source_url: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    embedding_status: row.get(5)?,
                    tagging_status: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(atoms)
    })();

    let atoms = match atoms_result {
        Ok(a) => a,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": format!("{}", e)}));
        }
    };

    // Build tag map
    let tag_map = match build_atom_tags_map(&conn) {
        Ok(m) => m,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e}));
        }
    };

    let mut result = Vec::new();
    for atom in atoms {
        let tags = tag_map.get(&atom.id).cloned().unwrap_or_default();
        let embedding = get_average_embedding(&conn, &atom.id);
        result.push(atomic_core::AtomWithEmbedding {
            atom: atomic_core::AtomWithTags { atom, tags },
            embedding,
        });
    }

    HttpResponse::Ok().json(result)
}

fn build_atom_tags_map(
    conn: &rusqlite::Connection,
) -> Result<std::collections::HashMap<String, Vec<atomic_core::Tag>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT at.atom_id, t.id, t.name, t.parent_id, t.created_at
             FROM atom_tags at
             INNER JOIN tags t ON at.tag_id = t.id
             ORDER BY t.name",
        )
        .map_err(|e| e.to_string())?;

    let mut map: std::collections::HashMap<String, Vec<atomic_core::Tag>> =
        std::collections::HashMap::new();

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                atomic_core::Tag {
                    id: row.get(1)?,
                    name: row.get(2)?,
                    parent_id: row.get(3)?,
                    created_at: row.get(4)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;

    for row in rows {
        let (atom_id, tag) = row.map_err(|e| e.to_string())?;
        map.entry(atom_id).or_default().push(tag);
    }

    Ok(map)
}

fn get_average_embedding(conn: &rusqlite::Connection, atom_id: &str) -> Option<Vec<f32>> {
    let mut stmt = conn
        .prepare("SELECT embedding FROM atom_chunks WHERE atom_id = ?1 AND embedding IS NOT NULL")
        .ok()?;

    let embeddings: Vec<Vec<u8>> = stmt
        .query_map([atom_id], |row| row.get(0))
        .ok()?
        .collect::<Result<Vec<_>, _>>()
        .ok()?;

    if embeddings.is_empty() {
        return None;
    }

    // Convert blobs to f32 vectors and average them
    let dim = embeddings[0].len() / 4;
    let mut avg = vec![0.0f32; dim];
    let count = embeddings.len() as f32;

    for blob in &embeddings {
        let floats: &[f32] =
            unsafe { std::slice::from_raw_parts(blob.as_ptr() as *const f32, dim) };
        for (i, &v) in floats.iter().enumerate() {
            avg[i] += v;
        }
    }

    for v in &mut avg {
        *v /= count;
    }

    Some(avg)
}
