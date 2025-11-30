use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

use crate::models::AtomCluster;

/// Compute clusters using a simplified label propagation algorithm.
/// This groups atoms that are highly connected via semantic edges.
pub fn compute_atom_clusters(
    conn: &Connection,
    min_similarity: f32,
    min_cluster_size: i32,
) -> Result<Vec<AtomCluster>, String> {
    // Load all semantic edges above threshold
    let mut stmt = conn
        .prepare(
            "SELECT source_atom_id, target_atom_id, similarity_score
             FROM semantic_edges
             WHERE similarity_score >= ?1",
        )
        .map_err(|e| e.to_string())?;

    let edges: Vec<(String, String, f32)> = stmt
        .query_map([min_similarity], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    if edges.is_empty() {
        return Ok(vec![]);
    }

    // Build adjacency list
    let mut adjacency: HashMap<String, Vec<(String, f32)>> = HashMap::new();
    let mut all_nodes: HashSet<String> = HashSet::new();

    for (source, target, score) in &edges {
        adjacency
            .entry(source.clone())
            .or_default()
            .push((target.clone(), *score));
        adjacency
            .entry(target.clone())
            .or_default()
            .push((source.clone(), *score));
        all_nodes.insert(source.clone());
        all_nodes.insert(target.clone());
    }

    // Initialize each node with its own cluster label
    let mut labels: HashMap<String, u32> = HashMap::new();
    for (i, node) in all_nodes.iter().enumerate() {
        labels.insert(node.clone(), i as u32);
    }

    // Label propagation: iterate until convergence or max iterations
    let max_iterations = 20;
    for _ in 0..max_iterations {
        let mut changed = false;

        for node in all_nodes.iter() {
            if let Some(neighbors) = adjacency.get(node) {
                // Count weighted votes for each neighbor's label
                let mut label_scores: HashMap<u32, f32> = HashMap::new();

                for (neighbor, weight) in neighbors {
                    if let Some(&neighbor_label) = labels.get(neighbor) {
                        *label_scores.entry(neighbor_label).or_default() += weight;
                    }
                }

                // Find the label with highest weighted vote
                if let Some((&best_label, _)) = label_scores
                    .iter()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                {
                    let current_label = *labels.get(node).unwrap();
                    if best_label != current_label {
                        labels.insert(node.clone(), best_label);
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    // Group nodes by their final labels
    let mut clusters_map: HashMap<u32, Vec<String>> = HashMap::new();
    for (node, label) in &labels {
        clusters_map.entry(*label).or_default().push(node.clone());
    }

    // Filter out small clusters and build result
    let mut clusters: Vec<AtomCluster> = Vec::new();
    let mut cluster_id = 0i32;

    for (_label, atom_ids) in clusters_map {
        if atom_ids.len() >= min_cluster_size as usize {
            // Get dominant tags for this cluster
            let dominant_tags = get_dominant_tags(conn, &atom_ids)?;

            clusters.push(AtomCluster {
                cluster_id,
                atom_ids,
                dominant_tags,
            });
            cluster_id += 1;
        }
    }

    // Sort clusters by size (largest first)
    clusters.sort_by(|a, b| b.atom_ids.len().cmp(&a.atom_ids.len()));

    // Re-assign IDs after sorting
    for (i, cluster) in clusters.iter_mut().enumerate() {
        cluster.cluster_id = i as i32;
    }

    Ok(clusters)
}

/// Get the most common tags in a set of atoms
fn get_dominant_tags(conn: &Connection, atom_ids: &[String]) -> Result<Vec<String>, String> {
    if atom_ids.is_empty() {
        return Ok(vec![]);
    }

    // Build placeholders for IN clause
    let placeholders: Vec<String> = atom_ids.iter().map(|_| "?".to_string()).collect();
    let placeholders_str = placeholders.join(",");

    let sql = format!(
        "SELECT t.name, COUNT(*) as cnt
         FROM atom_tags at
         JOIN tags t ON at.tag_id = t.id
         WHERE at.atom_id IN ({})
         GROUP BY t.id
         ORDER BY cnt DESC
         LIMIT 3",
        placeholders_str
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let params: Vec<&dyn rusqlite::ToSql> = atom_ids
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();

    let tags: Vec<String> = stmt
        .query_map(params.as_slice(), |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(tags)
}

/// Save cluster assignments to the database
pub fn save_cluster_assignments(
    conn: &Connection,
    clusters: &[AtomCluster],
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();

    // Clear existing assignments
    conn.execute("DELETE FROM atom_clusters", [])
        .map_err(|e| e.to_string())?;

    // Insert new assignments
    let mut stmt = conn
        .prepare("INSERT INTO atom_clusters (atom_id, cluster_id, computed_at) VALUES (?1, ?2, ?3)")
        .map_err(|e| e.to_string())?;

    for cluster in clusters {
        for atom_id in &cluster.atom_ids {
            stmt.execute(rusqlite::params![atom_id, cluster.cluster_id, now])
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Get cluster assignments from the database
pub fn get_cluster_assignments(conn: &Connection) -> Result<HashMap<String, u32>, String> {
    let mut stmt = conn
        .prepare("SELECT atom_id, cluster_id FROM atom_clusters")
        .map_err(|e| e.to_string())?;

    let assignments: HashMap<String, u32> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(assignments)
}

/// Calculate connection counts for hub identification
pub fn get_connection_counts(
    conn: &Connection,
    min_similarity: f32,
) -> Result<HashMap<String, i32>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT atom_id, COUNT(*) as cnt FROM (
                SELECT source_atom_id as atom_id FROM semantic_edges WHERE similarity_score >= ?1
                UNION ALL
                SELECT target_atom_id as atom_id FROM semantic_edges WHERE similarity_score >= ?1
            ) GROUP BY atom_id",
        )
        .map_err(|e| e.to_string())?;

    let counts: HashMap<String, i32> = stmt
        .query_map([min_similarity], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(counts)
}
