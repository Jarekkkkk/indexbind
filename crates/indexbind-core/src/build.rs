use crate::chunking::ChunkingOptions;
use crate::embedding::EmbeddingBackend;
use crate::types::{NormalizedDocument, SourceRoot};
use blake3::Hasher;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct BuildArtifactOptions {
    pub source_root: SourceRoot,
    pub embedding_backend: EmbeddingBackend,
    pub chunking: ChunkingOptions,
}

impl Default for BuildArtifactOptions {
    fn default() -> Self {
        Self {
            source_root: SourceRoot {
                id: "root".to_string(),
                original_path: ".".to_string(),
            },
            embedding_backend: EmbeddingBackend::default(),
            chunking: ChunkingOptions::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildStats {
    pub document_count: usize,
    pub chunk_count: usize,
}

#[derive(Debug, Clone)]
pub struct IncrementalBuildStats {
    pub scanned_document_count: usize,
    pub new_document_count: usize,
    pub changed_document_count: usize,
    pub unchanged_document_count: usize,
    pub removed_document_count: usize,
    pub active_document_count: usize,
    pub active_chunk_count: usize,
}

pub(crate) fn build_doc_id(source_root_id: &str, relative_path: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(source_root_id.as_bytes());
    hasher.update(b":");
    hasher.update(relative_path.as_bytes());
    hasher.finalize().to_hex().to_string()
}

pub(crate) fn build_chunk_id(doc_id: &str, ordinal: usize) -> i64 {
    let mut hasher = Hasher::new();
    hasher.update(doc_id.as_bytes());
    hasher.update(b":");
    hasher.update(ordinal.to_string().as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    i64::from_be_bytes(bytes) & i64::MAX
}

pub(crate) fn build_content_hash(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

pub(crate) fn build_document_fingerprint(document: &NormalizedDocument) -> String {
    build_hash_from_bytes(
        &serde_json::to_vec(&json!({
            "relative_path": document.relative_path,
            "canonical_url": document.canonical_url,
            "title": document.title,
            "summary": document.summary,
            "content": document.content,
            "metadata": document.metadata,
        }))
        .unwrap_or_default(),
    )
}

pub(crate) fn build_chunking_fingerprint(options: &ChunkingOptions) -> String {
    build_hash_from_bytes(
        &serde_json::to_vec(&json!({
            "target_tokens": options.target_tokens,
            "overlap_tokens": options.overlap_tokens,
        }))
        .unwrap_or_default(),
    )
}

pub(crate) fn build_embedding_backend_fingerprint(backend: &EmbeddingBackend) -> String {
    build_hash_from_bytes(&serde_json::to_vec(backend).unwrap_or_default())
}

fn build_hash_from_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
