//! Memory management: markdown files → chunked → embedded → hybrid search in SQLite.

pub mod chunker;
pub mod config;
pub mod embeddings;
pub mod embeddings_batch;
pub mod embeddings_fallback;
#[cfg(feature = "local-embeddings")]
pub mod embeddings_local;
pub mod embeddings_openai;
pub mod manager;
pub mod schema;
pub mod search;
pub mod store;
pub mod store_sqlite;
pub mod tools;
#[cfg(feature = "file-watcher")]
pub mod watcher;
