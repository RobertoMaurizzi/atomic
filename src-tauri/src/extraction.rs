//! Extraction module - re-exports from atomic-core

pub use atomic_core::extraction::{
    get_tag_tree_for_llm, link_tags_to_atom, tag_names_to_ids,
    get_or_create_tag, cleanup_orphaned_parents, build_tag_info_for_consolidation,
    extract_tags_from_chunk, consolidate_atom_tags,
};
