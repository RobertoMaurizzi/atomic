//! Wiki module - re-exports from atomic-core

pub use atomic_core::wiki::{
    prepare_wiki_generation, prepare_wiki_update,
    generate_wiki_content, update_wiki_content,
    save_wiki_article, load_wiki_article, get_article_status,
    delete_article, load_all_wiki_articles,
};
