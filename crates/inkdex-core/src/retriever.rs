use crate::embedding::{bytes_to_vector, cosine_similarity, Embedder, EmbeddingBackend};
use crate::types::{BestMatch, DocumentHit, LoadedDocument, SourceRoot, StoredChunk, StoredDocument};
use crate::{InkdexError, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub schema_version: String,
    pub built_at: String,
    pub embedding_backend: EmbeddingBackend,
    pub source_root: SourceRoot,
    pub document_count: usize,
    pub chunk_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub top_k: usize,
    pub hybrid: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            top_k: 10,
            hybrid: true,
        }
    }
}

#[derive(Debug, Clone)]
struct IndexedChunk {
    chunk: StoredChunk,
    embedding: Vec<f32>,
}

pub struct Retriever {
    info: ArtifactInfo,
    documents: HashMap<String, StoredDocument>,
    chunks: Vec<IndexedChunk>,
    source_root_override: Option<PathBuf>,
    embedder: Embedder,
}

impl Retriever {
    pub fn open(path: &Path, source_root_override: Option<PathBuf>) -> Result<Self> {
        let connection = Connection::open(path)?;
        let info = load_info(&connection)?;
        let documents = load_documents(&connection)?;
        let chunks = load_chunks(&connection)?;
        let embedder = Embedder::new(info.embedding_backend.clone())?;

        Ok(Self {
            info,
            documents,
            chunks,
            source_root_override,
            embedder,
        })
    }

    pub fn info(&self) -> &ArtifactInfo {
        &self.info
    }

    pub fn search(&mut self, query: &str, options: SearchOptions) -> Result<Vec<DocumentHit>> {
        let query_embedding = self
            .embedder
            .embed_queries(&[query.to_string()])?
            .into_iter()
            .next()
            .unwrap_or_default();
        let query_terms = tokenize(query);

        let mut by_document: HashMap<String, Vec<(f32, &StoredChunk)>> = HashMap::new();
        for indexed_chunk in &self.chunks {
            let vector_score = cosine_similarity(&query_embedding, &indexed_chunk.embedding);
            let lexical_score = if options.hybrid {
                lexical_score(&query_terms, &indexed_chunk.chunk.chunk_text)
            } else {
                0.0
            };
            let score = if options.hybrid {
                (0.7 * vector_score) + (0.3 * lexical_score)
            } else {
                vector_score
            };
            if score <= 0.0 {
                continue;
            }
            by_document
                .entry(indexed_chunk.chunk.doc_id.clone())
                .or_default()
                .push((score, &indexed_chunk.chunk));
        }

        let mut hits = by_document
            .into_iter()
            .filter_map(|(doc_id, mut scores)| {
                scores.sort_by(|left, right| right.0.partial_cmp(&left.0).unwrap_or(Ordering::Equal));
                let best = scores.first()?;
                let stored_document = self.documents.get(&doc_id)?;
                let aggregate = best.0 + scores.iter().skip(1).take(2).map(|entry| entry.0).sum::<f32>() * 0.1;
                Some(DocumentHit {
                    doc_id: stored_document.doc_id.clone(),
                    original_path: stored_document.original_path.clone(),
                    relative_path: stored_document.relative_path.clone(),
                    title: stored_document.title.clone(),
                    score: aggregate,
                    best_match: BestMatch {
                        chunk_id: best.1.chunk_id,
                        excerpt: best.1.excerpt.clone(),
                        heading_path: best.1.heading_path.clone(),
                        char_start: best.1.char_start,
                        char_end: best.1.char_end,
                        score: best.0,
                    },
                    metadata: stored_document.metadata.clone(),
                })
            })
            .collect::<Vec<_>>();

        hits.sort_by(|left, right| right.score.partial_cmp(&left.score).unwrap_or(Ordering::Equal));
        hits.truncate(options.top_k);
        Ok(hits)
    }

    pub fn read_document(&self, hit: &DocumentHit) -> Result<LoadedDocument> {
        let path = if let Some(source_root) = &self.source_root_override {
            source_root.join(&hit.relative_path)
        } else {
            PathBuf::from(&hit.original_path)
        };
        let content = fs::read_to_string(&path)
            .map_err(|_| InkdexError::DocumentNotFound(path.display().to_string()))?;
        Ok(LoadedDocument {
            original_path: hit.original_path.clone(),
            relative_path: hit.relative_path.clone(),
            content,
        })
    }
}

fn load_info(connection: &Connection) -> Result<ArtifactInfo> {
    let mut statement = connection.prepare("SELECT key, value FROM artifact_meta")?;
    let mut rows = statement.query([])?;
    let mut values = HashMap::new();
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        values.insert(key, value);
    }

    let schema_version = values
        .remove("schema_version")
        .ok_or(InkdexError::MissingMetadata("schema_version"))?;
    let built_at = values
        .remove("built_at")
        .ok_or(InkdexError::MissingMetadata("built_at"))?;
    let embedding_backend = serde_json::from_str(
        values
            .get("embedding_backend")
            .ok_or(InkdexError::MissingMetadata("embedding_backend"))?,
    )?;
    let source_root = serde_json::from_str(
        values
            .get("source_root")
            .ok_or(InkdexError::MissingMetadata("source_root"))?,
    )?;

    let document_count = connection.query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;
    let chunk_count = connection.query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;

    Ok(ArtifactInfo {
        schema_version,
        built_at,
        embedding_backend,
        source_root,
        document_count,
        chunk_count,
    })
}

fn load_documents(connection: &Connection) -> Result<HashMap<String, StoredDocument>> {
    let mut statement = connection.prepare(
        "SELECT doc_id, source_root_id, original_path, relative_path, title, content_hash, modified_at, chunk_count, metadata_json FROM documents",
    )?;
    let documents = statement
        .query_map([], |row| {
            let metadata_json: String = row.get(8)?;
            Ok(StoredDocument {
                doc_id: row.get(0)?,
                source_root_id: row.get(1)?,
                original_path: row.get(2)?,
                relative_path: row.get(3)?,
                title: row.get(4)?,
                content_hash: row.get(5)?,
                modified_at: row.get(6)?,
                chunk_count: row.get::<_, i64>(7)? as usize,
                metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(documents
        .into_iter()
        .map(|document| (document.doc_id.clone(), document))
        .collect())
}

fn load_chunks(connection: &Connection) -> Result<Vec<IndexedChunk>> {
    let mut statement = connection.prepare(
        "
        SELECT
            c.chunk_id,
            c.doc_id,
            c.ordinal,
            c.heading_path_json,
            c.char_start,
            c.char_end,
            c.token_count,
            c.chunk_text,
            c.excerpt,
            v.vector_blob
        FROM chunks c
        INNER JOIN chunk_vectors v ON v.chunk_id = c.chunk_id
        ",
    )?;
    let chunks = statement
        .query_map([], |row| {
            let heading_path_json: String = row.get(3)?;
            let vector_blob: Vec<u8> = row.get(9)?;
            Ok(IndexedChunk {
                chunk: StoredChunk {
                    chunk_id: row.get(0)?,
                    doc_id: row.get(1)?,
                    ordinal: row.get::<_, i64>(2)? as usize,
                    heading_path: serde_json::from_str(&heading_path_json).unwrap_or_default(),
                    char_start: row.get::<_, i64>(4)? as usize,
                    char_end: row.get::<_, i64>(5)? as usize,
                    token_count: row.get::<_, i64>(6)? as usize,
                    chunk_text: row.get(7)?,
                    excerpt: row.get(8)?,
                },
                embedding: bytes_to_vector(&vector_blob),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(chunks)
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_lowercase())
        .collect()
}

fn lexical_score(query_terms: &[String], haystack: &str) -> f32 {
    if query_terms.is_empty() {
        return 0.0;
    }
    let haystack = haystack.to_lowercase();
    let matches = query_terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count();
    matches as f32 / query_terms.len() as f32
}

#[cfg(test)]
mod tests {
    use super::{Retriever, SearchOptions};
    use crate::artifact::{build_artifact, BuildArtifactOptions};
    use crate::embedding::EmbeddingBackend;
    use crate::types::{NormalizedDocument, SourceRoot};
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    #[test]
    fn returns_document_hits_and_reads_source_content() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("docs");
        std::fs::create_dir_all(&source).unwrap();
        let file = source.join("guide.md");
        std::fs::write(&file, "# Intro\nRust embeddings and retrieval.").unwrap();

        let artifact = dir.path().join("index.sqlite");
        build_artifact(
            &artifact,
            &[NormalizedDocument {
                original_path: file.display().to_string(),
                relative_path: "guide.md".to_string(),
                title: Some("Intro".to_string()),
                content: "# Intro\nRust embeddings and retrieval.".to_string(),
                metadata: BTreeMap::new(),
            }],
            &BuildArtifactOptions {
                source_root: SourceRoot {
                    id: "root".to_string(),
                    original_path: source.display().to_string(),
                },
                embedding_backend: EmbeddingBackend::Hashing { dimensions: 128 },
                chunking: Default::default(),
            },
        )
        .unwrap();

        let mut retriever = Retriever::open(&artifact, None).unwrap();
        let hits = retriever
            .search("rust retrieval", SearchOptions::default())
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert!(hits[0].original_path.ends_with("guide.md"));
        let loaded = retriever.read_document(&hits[0]).unwrap();
        assert!(loaded.content.contains("Rust embeddings"));
    }
}
