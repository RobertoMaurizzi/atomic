//! Compaction module - re-exports from atomic-core

pub use atomic_core::compaction::{
    CompactionResult,
    apply_merge_operations, read_all_tags, fetch_merge_suggestions,
};
