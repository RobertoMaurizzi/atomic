use super::SqliteStorage;
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::storage::traits::*;
use crate::{
    atom_from_row, extract_title_and_snippet, get_all_atom_tags_map, get_all_average_embeddings,
    get_atom_tags_map_for_ids, get_tags_for_atom, parse_source, CreateAtomRequest, ListAtomsParams,
    UpdateAtomRequest, ATOM_COLUMNS, ATOM_COLUMNS_A,
};
use async_trait::async_trait;

impl SqliteStorage {
    pub(crate) fn get_all_atoms_impl(&self) -> StorageResult<Vec<AtomWithTags>> {
        let conn = self.db.read_conn()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM atoms ORDER BY updated_at DESC",
            ATOM_COLUMNS
        ))?;

        let atoms: Vec<Atom> = stmt
            .query_map([], atom_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        let tag_map = get_all_atom_tags_map(&conn)?;

        let result: Vec<AtomWithTags> = atoms
            .into_iter()
            .map(|atom| {
                let tags = tag_map.get(&atom.id).cloned().unwrap_or_default();
                AtomWithTags { atom, tags }
            })
            .collect();

        Ok(result)
    }

    pub(crate) fn get_atom_impl(&self, id: &str) -> StorageResult<Option<AtomWithTags>> {
        let conn = self.db.read_conn()?;

        let atom_result = conn.query_row(
            &format!("SELECT {} FROM atoms WHERE id = ?1", ATOM_COLUMNS),
            [id],
            atom_from_row,
        );

        match atom_result {
            Ok(atom) => {
                let tags = get_tags_for_atom(&conn, id)?;
                Ok(Some(AtomWithTags { atom, tags }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AtomicCoreError::Database(e)),
        }
    }

    pub(crate) fn insert_atom_impl(
        &self,
        id: &str,
        request: &CreateAtomRequest,
        created_at: &str,
    ) -> StorageResult<AtomWithTags> {
        let embedding_status = "pending";
        let (title, snippet) = extract_title_and_snippet(&request.content, 300);
        let source = request.source_url.as_deref().map(parse_source);

        {
            let conn = self
                .db
                .conn
                .lock()
                .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

            conn.execute(
                "INSERT INTO atoms (id, content, source_url, source, published_at, created_at, updated_at, embedding_status, title, snippet)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                (
                    id,
                    &request.content,
                    &request.source_url,
                    &source,
                    &request.published_at,
                    created_at,
                    created_at,
                    &embedding_status,
                    &title,
                    &snippet,
                ),
            )?;

            for tag_id in &request.tag_ids {
                conn.execute(
                    "INSERT INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
                    (id, tag_id),
                )?;
            }
        }

        let atom = Atom {
            id: id.to_string(),
            content: request.content.clone(),
            title,
            snippet,
            source_url: request.source_url.clone(),
            source,
            published_at: request.published_at.clone(),
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
            embedding_status: embedding_status.to_string(),
            tagging_status: "pending".to_string(),
        };

        let tags = {
            let conn = self
                .db
                .conn
                .lock()
                .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            get_tags_for_atom(&conn, id)?
        };

        Ok(AtomWithTags { atom, tags })
    }

    pub(crate) fn insert_atoms_bulk_impl(
        &self,
        atoms: &[(String, CreateAtomRequest, String)],
    ) -> StorageResult<Vec<AtomWithTags>> {
        let mut atoms_with_tags: Vec<AtomWithTags> = Vec::with_capacity(atoms.len());

        {
            let conn = self
                .db
                .conn
                .lock()
                .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

            conn.execute_batch("BEGIN")?;

            for (id, request, created_at) in atoms {
                let (title, snippet) = extract_title_and_snippet(&request.content, 300);
                let source = request.source_url.as_deref().map(parse_source);

                if let Err(e) = conn.execute(
                    "INSERT INTO atoms (id, content, source_url, source, published_at, created_at, updated_at, embedding_status, title, snippet)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    (
                        id,
                        &request.content,
                        &request.source_url,
                        &source,
                        &request.published_at,
                        created_at,
                        created_at,
                        &"pending",
                        &title,
                        &snippet,
                    ),
                ) {
                    conn.execute_batch("ROLLBACK")?;
                    return Err(AtomicCoreError::Database(e));
                }

                for tag_id in &request.tag_ids {
                    if let Err(e) = conn.execute(
                        "INSERT INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
                        (id, tag_id),
                    ) {
                        conn.execute_batch("ROLLBACK")?;
                        return Err(AtomicCoreError::Database(e));
                    }
                }

                let atom = Atom {
                    id: id.clone(),
                    content: request.content.clone(),
                    title,
                    snippet,
                    source_url: request.source_url.clone(),
                    source,
                    published_at: request.published_at.clone(),
                    created_at: created_at.clone(),
                    updated_at: created_at.clone(),
                    embedding_status: "pending".to_string(),
                    tagging_status: "pending".to_string(),
                };

                atoms_with_tags.push(AtomWithTags {
                    atom,
                    tags: vec![],
                });
            }

            conn.execute_batch("COMMIT")?;

            // Resolve tags for each atom after commit
            for atom_with_tags in &mut atoms_with_tags {
                atom_with_tags.tags = get_tags_for_atom(&conn, &atom_with_tags.atom.id)?;
            }
        }

        Ok(atoms_with_tags)
    }

    pub(crate) fn update_atom_impl(
        &self,
        id: &str,
        request: &UpdateAtomRequest,
        updated_at: &str,
    ) -> StorageResult<AtomWithTags> {
        let embedding_status = "pending";
        let (title, snippet) = extract_title_and_snippet(&request.content, 300);
        let source = request.source_url.as_deref().map(parse_source);

        {
            let conn = self
                .db
                .conn
                .lock()
                .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

            conn.execute(
                "UPDATE atoms SET content = ?1, source_url = ?2, source = ?3, published_at = ?4, updated_at = ?5, embedding_status = ?6,
                 title = ?7, snippet = ?8
                 WHERE id = ?9",
                (
                    &request.content,
                    &request.source_url,
                    &source,
                    &request.published_at,
                    updated_at,
                    &embedding_status,
                    &title,
                    &snippet,
                    id,
                ),
            )?;

            // Remove existing tags and add new ones (only if tag_ids provided)
            if let Some(ref tag_ids) = request.tag_ids {
                conn.execute("DELETE FROM atom_tags WHERE atom_id = ?1", [id])?;

                for tag_id in tag_ids {
                    conn.execute(
                        "INSERT INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
                        (id, tag_id),
                    )?;
                }
            }
        }

        // Get the updated atom
        let atom = {
            let conn = self
                .db
                .conn
                .lock()
                .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            conn.query_row(
                &format!("SELECT {} FROM atoms WHERE id = ?1", ATOM_COLUMNS),
                [id],
                atom_from_row,
            )?
        };

        let tags = {
            let conn = self
                .db
                .conn
                .lock()
                .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            get_tags_for_atom(&conn, id)?
        };

        Ok(AtomWithTags { atom, tags })
    }

    pub(crate) fn delete_atom_impl(&self, id: &str) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        // Explicit delete from atom_tags so the trigger decrements tags.atom_count.
        // (FK CASCADE is off, so this won't happen automatically.)
        conn.execute("DELETE FROM atom_tags WHERE atom_id = ?1", [id])?;
        conn.execute("DELETE FROM atoms WHERE id = ?1", [id])?;

        Ok(())
    }

    pub(crate) fn get_atoms_by_tag_impl(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<AtomWithTags>> {
        let conn = self.db.read_conn()?;

        let mut stmt = conn.prepare(&format!(
            "WITH RECURSIVE descendant_tags(id) AS (
                SELECT ?1
                UNION ALL
                SELECT t.id FROM tags t
                INNER JOIN descendant_tags dt ON t.parent_id = dt.id
            )
            SELECT {ATOM_COLUMNS_A}
            FROM atom_tags at
            INNER JOIN atoms a ON a.id = at.atom_id
            WHERE at.tag_id IN (SELECT id FROM descendant_tags)
            GROUP BY a.id
            ORDER BY a.updated_at DESC",
        ))?;

        let atoms: Vec<Atom> = stmt
            .query_map(rusqlite::params![tag_id], atom_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        // Batch load tags for the fetched atoms
        let atom_ids: Vec<String> = atoms.iter().map(|a| a.id.clone()).collect();
        let tag_map = get_atom_tags_map_for_ids(&conn, &atom_ids)?;

        let result: Vec<AtomWithTags> = atoms
            .into_iter()
            .map(|atom| {
                let tags = tag_map.get(&atom.id).cloned().unwrap_or_default();
                AtomWithTags { atom, tags }
            })
            .collect();

        Ok(result)
    }

    pub(crate) fn list_atoms_impl(
        &self,
        params: &ListAtomsParams,
    ) -> StorageResult<PaginatedAtoms> {
        let conn = self.db.read_conn()?;
        let use_cursor = params.cursor.is_some() && params.cursor_id.is_some();

        // Determine if non-tag filters are active (source filters bypass atom_count shortcut)
        let has_extra_filters = !matches!(params.source_filter, SourceFilter::All)
            || params.source_value.is_some();

        // --- Build ORDER BY ---
        let sort_col = match params.sort_by {
            SortField::Updated => "a.updated_at",
            SortField::Created => "a.created_at",
            SortField::Published => "COALESCE(a.published_at, a.created_at)",
            SortField::Title => "a.title",
        };
        let sort_dir = match params.sort_order {
            SortOrder::Desc => "DESC",
            SortOrder::Asc => "ASC",
        };
        let cursor_cmp = match params.sort_order {
            SortOrder::Desc => "<",
            SortOrder::Asc => ">",
        };

        // --- Build WHERE clauses + bind values ---
        let mut where_clauses: Vec<String> = Vec::new();
        let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        // Tag filter
        if let Some(ref tid) = params.tag_id {
            where_clauses.push(format!(
                "EXISTS (SELECT 1 FROM atom_tags at WHERE at.atom_id = a.id AND at.tag_id IN (SELECT id FROM tags WHERE id = ?{p} OR parent_id = ?{p}))",
                p = param_idx
            ));
            bind_values.push(Box::new(tid.clone()));
            param_idx += 1;
        }

        // Source filter
        match params.source_filter {
            SourceFilter::All => {}
            SourceFilter::Manual => {
                where_clauses.push("a.source IS NULL".to_string());
            }
            SourceFilter::External => {
                where_clauses.push("a.source IS NOT NULL".to_string());
            }
        }

        // Source value filter (specific source like "nytimes.com")
        if let Some(ref sv) = params.source_value {
            where_clauses.push(format!("a.source = ?{}", param_idx));
            bind_values.push(Box::new(sv.clone()));
            param_idx += 1;
        }

        // Cursor
        if use_cursor {
            where_clauses.push(format!(
                "({sort_col}, a.id) {cursor_cmp} (?{p1}, ?{p2})",
                sort_col = sort_col,
                cursor_cmp = cursor_cmp,
                p1 = param_idx,
                p2 = param_idx + 1,
            ));
            bind_values.push(Box::new(params.cursor.clone().unwrap()));
            bind_values.push(Box::new(params.cursor_id.clone().unwrap()));
            param_idx += 2;
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        // --- Count query ---
        let total_count: i32 = if !has_extra_filters && params.tag_id.is_some() {
            // Fast path: use denormalized atom_count for tag-only filters
            let tid = params.tag_id.as_ref().unwrap();
            let has_children: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM tags WHERE parent_id = ?1)",
                rusqlite::params![tid],
                |row| row.get(0),
            )?;
            if has_children {
                conn.query_row(
                    "SELECT COUNT(DISTINCT at.atom_id)
                     FROM atom_tags at
                     WHERE at.tag_id IN (SELECT id FROM tags WHERE id = ?1 OR parent_id = ?1)",
                    rusqlite::params![tid],
                    |row| row.get(0),
                )?
            } else {
                conn.query_row(
                    "SELECT atom_count FROM tags WHERE id = ?1",
                    rusqlite::params![tid],
                    |row| row.get(0),
                )?
            }
        } else if has_extra_filters || params.tag_id.is_some() {
            // Build count query with filters (no cursor/limit)
            let mut count_wheres: Vec<String> = Vec::new();
            let mut count_binds: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut ci = 1;

            if let Some(ref tid) = params.tag_id {
                count_wheres.push(format!(
                    "EXISTS (SELECT 1 FROM atom_tags at WHERE at.atom_id = a.id AND at.tag_id IN (SELECT id FROM tags WHERE id = ?{p} OR parent_id = ?{p}))",
                    p = ci
                ));
                count_binds.push(Box::new(tid.clone()));
                ci += 1;
            }
            match params.source_filter {
                SourceFilter::All => {}
                SourceFilter::Manual => count_wheres.push("a.source IS NULL".to_string()),
                SourceFilter::External => count_wheres.push("a.source IS NOT NULL".to_string()),
            }
            if let Some(ref sv) = params.source_value {
                count_wheres.push(format!("a.source = ?{}", ci));
                count_binds.push(Box::new(sv.clone()));
                // ci += 1;
            }
            let count_where = if count_wheres.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", count_wheres.join(" AND "))
            };
            let count_sql = format!("SELECT COUNT(*) FROM atoms a {}", count_where);
            let count_refs: Vec<&dyn rusqlite::types::ToSql> =
                count_binds.iter().map(|b| b.as_ref()).collect();
            conn.query_row(&count_sql, count_refs.as_slice(), |row| row.get(0))?
        } else {
            // No filters at all -- plain count
            conn.query_row("SELECT COUNT(*) FROM atoms", [], |row| row.get(0))?
        };

        // --- Data query ---
        // Bind values for LIMIT/OFFSET after cursor
        let limit_param = param_idx;
        bind_values.push(Box::new(params.limit));
        param_idx += 1;

        let data_sql = if use_cursor {
            format!(
                "SELECT a.id, a.title, a.snippet, a.source_url, a.source, a.published_at,
                        a.created_at, a.updated_at,
                        COALESCE(a.embedding_status, 'pending'), COALESCE(a.tagging_status, 'pending')
                 FROM atoms a
                 {where_sql}
                 ORDER BY {sort_col} {sort_dir}, a.id {sort_dir}
                 LIMIT ?{limit_param}",
            )
        } else {
            let offset_param = param_idx;
            bind_values.push(Box::new(params.offset));
            // param_idx += 1;
            format!(
                "SELECT a.id, a.title, a.snippet, a.source_url, a.source, a.published_at,
                        a.created_at, a.updated_at,
                        COALESCE(a.embedding_status, 'pending'), COALESCE(a.tagging_status, 'pending')
                 FROM atoms a
                 {where_sql}
                 ORDER BY {sort_col} {sort_dir}, a.id {sort_dir}
                 LIMIT ?{limit_param} OFFSET ?{offset_param}",
            )
        };

        let bind_refs: Vec<&dyn rusqlite::types::ToSql> =
            bind_values.iter().map(|b| b.as_ref()).collect();
        let mut stmt = conn.prepare(&data_sql)?;
        type AtomRow = (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            String,
        );
        let atoms: Vec<AtomRow> = stmt
            .query_map(bind_refs.as_slice(), |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Batch load tags for the page
        let atom_ids: Vec<String> = atoms.iter().map(|a| a.0.clone()).collect();
        let tag_map = get_atom_tags_map_for_ids(&conn, &atom_ids)?;

        // Extract cursor from the last result for keyset pagination.
        let (next_cursor, next_cursor_id) = atoms
            .last()
            .map(|last| (Some(last.7.clone()), Some(last.0.clone())))
            .unwrap_or((None, None));

        let summaries: Vec<AtomSummary> = atoms
            .into_iter()
            .map(
                |(
                    id,
                    title,
                    snippet,
                    source_url,
                    source,
                    published_at,
                    created_at,
                    updated_at,
                    embedding_status,
                    tagging_status,
                )| {
                    let tags = tag_map.get(&id).cloned().unwrap_or_default();
                    AtomSummary {
                        id,
                        title,
                        snippet,
                        source_url,
                        source,
                        published_at,
                        created_at,
                        updated_at,
                        embedding_status,
                        tagging_status,
                        tags,
                    }
                },
            )
            .collect();

        Ok(PaginatedAtoms {
            atoms: summaries,
            total_count,
            limit: params.limit,
            offset: params.offset,
            next_cursor,
            next_cursor_id,
        })
    }

    pub(crate) fn get_source_list_impl(&self) -> StorageResult<Vec<SourceInfo>> {
        let conn = self.db.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT source, COUNT(*) as cnt FROM atoms WHERE source IS NOT NULL GROUP BY source ORDER BY cnt DESC",
        )?;
        let results = stmt
            .query_map([], |row| {
                Ok(SourceInfo {
                    source: row.get(0)?,
                    atom_count: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub(crate) fn get_embedding_status_impl(&self, atom_id: &str) -> StorageResult<String> {
        let conn = self.db.read_conn()?;

        let status: String = conn.query_row(
            "SELECT COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
            [atom_id],
            |row| row.get(0),
        )?;

        Ok(status)
    }

    pub(crate) fn get_atom_positions_impl(&self) -> StorageResult<Vec<AtomPosition>> {
        let conn = self.db.read_conn()?;

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
    }

    pub(crate) fn save_atom_positions_impl(
        &self,
        positions: &[AtomPosition],
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        let tx = conn.unchecked_transaction()?;
        for pos in positions {
            tx.execute(
                "INSERT OR REPLACE INTO atom_positions (atom_id, x, y, updated_at) VALUES (?1, ?2, ?3, ?4)",
                (&pos.atom_id, &pos.x, &pos.y, &now),
            )?;
        }
        tx.commit()?;

        Ok(())
    }

    pub(crate) fn get_atoms_with_embeddings_impl(
        &self,
    ) -> StorageResult<Vec<AtomWithEmbedding>> {
        let conn = self.db.read_conn()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM atoms ORDER BY updated_at DESC",
            ATOM_COLUMNS
        ))?;

        let atoms: Vec<Atom> = stmt
            .query_map([], atom_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        let tag_map = get_all_atom_tags_map(&conn)?;

        // Batch-load all embeddings in a single query
        let embedding_map = get_all_average_embeddings(&conn)?;

        let result = atoms
            .into_iter()
            .map(|atom| {
                let tags = tag_map.get(&atom.id).cloned().unwrap_or_default();
                let embedding = embedding_map.get(&atom.id).cloned();
                AtomWithEmbedding {
                    atom: AtomWithTags { atom, tags },
                    embedding,
                }
            })
            .collect();

        Ok(result)
    }
}

#[async_trait]
impl AtomStore for SqliteStorage {
    async fn get_all_atoms(&self) -> StorageResult<Vec<AtomWithTags>> {
        self.get_all_atoms_impl()
    }

    async fn get_atom(&self, id: &str) -> StorageResult<Option<AtomWithTags>> {
        self.get_atom_impl(id)
    }

    async fn insert_atom(
        &self,
        id: &str,
        request: &CreateAtomRequest,
        created_at: &str,
    ) -> StorageResult<AtomWithTags> {
        self.insert_atom_impl(id, request, created_at)
    }

    async fn insert_atoms_bulk(
        &self,
        atoms: &[(String, CreateAtomRequest, String)],
    ) -> StorageResult<Vec<AtomWithTags>> {
        self.insert_atoms_bulk_impl(atoms)
    }

    async fn update_atom(
        &self,
        id: &str,
        request: &UpdateAtomRequest,
        updated_at: &str,
    ) -> StorageResult<AtomWithTags> {
        self.update_atom_impl(id, request, updated_at)
    }

    async fn delete_atom(&self, id: &str) -> StorageResult<()> {
        self.delete_atom_impl(id)
    }

    async fn get_atoms_by_tag(&self, tag_id: &str) -> StorageResult<Vec<AtomWithTags>> {
        self.get_atoms_by_tag_impl(tag_id)
    }

    async fn list_atoms(&self, params: &ListAtomsParams) -> StorageResult<PaginatedAtoms> {
        self.list_atoms_impl(params)
    }

    async fn get_source_list(&self) -> StorageResult<Vec<SourceInfo>> {
        self.get_source_list_impl()
    }

    async fn get_embedding_status(&self, atom_id: &str) -> StorageResult<String> {
        self.get_embedding_status_impl(atom_id)
    }

    async fn get_atom_positions(&self) -> StorageResult<Vec<AtomPosition>> {
        self.get_atom_positions_impl()
    }

    async fn save_atom_positions(&self, positions: &[AtomPosition]) -> StorageResult<()> {
        self.save_atom_positions_impl(positions)
    }

    async fn get_atoms_with_embeddings(&self) -> StorageResult<Vec<AtomWithEmbedding>> {
        self.get_atoms_with_embeddings_impl()
    }
}
