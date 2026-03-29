use crate::artifact::initialize_schema as initialize_artifact_schema;
use crate::build::{
    build_chunk_id, build_chunking_fingerprint, build_content_hash, build_doc_id,
    build_document_fingerprint, build_embedding_backend_fingerprint, BuildArtifactOptions,
    BuildStats, IncrementalBuildStats,
};
use crate::canonical::{
    build_postings, maybe_write_model_assets, CanonicalArtifactFiles, CanonicalArtifactManifest,
    CanonicalBuildStats, CanonicalChunkRecord, CanonicalChunkingConfig, CanonicalDocumentRecord,
};
use crate::chunking::chunk_document;
use crate::embedding::{format_chunk_for_embedding, vector_to_bytes, Embedder, EmbeddingBackend};
use crate::lexical::{tokenize_for_storage, LEXICAL_TOKENIZER_VERSION};
use crate::types::{NormalizedDocument, SourceRoot, StoredChunk, StoredDocument};
use crate::{IndexbindError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_SCHEMA_VERSION: &str = "1";

#[derive(Debug, Clone, Default)]
pub struct BuildCacheUpdate {
    pub documents: Vec<NormalizedDocument>,
    pub removed_relative_paths: Vec<String>,
    pub replace_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCacheInfo {
    pub schema_version: String,
    pub source_root: SourceRoot,
    pub embedding_backend: EmbeddingBackend,
    pub lexical_tokenizer: String,
    pub chunking: CacheChunkingInfo,
    pub document_count: usize,
    pub chunk_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheChunkingInfo {
    pub target_tokens: usize,
    pub overlap_tokens: usize,
}

struct MaterializedDocument {
    stored_document: StoredDocument,
    document_fingerprint: String,
    chunks: Vec<MaterializedChunk>,
}

struct MaterializedChunk {
    stored_chunk: StoredChunk,
    dimensions: usize,
    vector_blob: Vec<u8>,
    lexical_text: String,
    lexical_excerpt: String,
}

#[derive(Debug)]
struct CachedDocumentFingerprint {
    relative_path: String,
    document_fingerprint: String,
    is_deleted: bool,
}

pub fn update_build_cache(
    cache_path: &Path,
    update: BuildCacheUpdate,
    options: &BuildArtifactOptions,
) -> Result<IncrementalBuildStats> {
    let mut embedder = Embedder::new(options.embedding_backend.clone())?;
    let mut connection = Connection::open(cache_path)?;
    initialize_cache_schema(&connection)?;
    let config_changed = refresh_cache_meta(&connection, options)?;
    if config_changed {
        clear_cache_contents(&connection)?;
    }

    let existing = load_cached_document_fingerprints(&connection)?;
    let mut existing_by_path = BTreeMap::new();
    for row in existing {
        existing_by_path.insert(row.relative_path.clone(), row);
    }

    let BuildCacheUpdate {
        documents,
        removed_relative_paths,
        replace_all,
    } = update;
    let mut scanned_document_count = 0usize;
    let mut new_document_count = 0usize;
    let mut changed_document_count = 0usize;
    let mut unchanged_document_count = 0usize;
    let mut removed_document_count = 0usize;
    let mut seen_relative_paths = BTreeSet::new();

    let transaction = connection.transaction()?;
    for document in documents {
        scanned_document_count += 1;
        seen_relative_paths.insert(document.relative_path.clone());
        let document_fingerprint = build_document_fingerprint(&document);
        match existing_by_path.get(&document.relative_path) {
            Some(existing)
                if !config_changed
                    && !existing.is_deleted
                    && existing.document_fingerprint == document_fingerprint =>
            {
                unchanged_document_count += 1;
                continue;
            }
            Some(_) => changed_document_count += 1,
            None => new_document_count += 1,
        }

        let materialized = materialize_document(&document, options, &mut embedder)?;
        upsert_materialized_document(&transaction, &materialized, &options.source_root.id)?;
    }

    let mut relative_paths_to_remove = BTreeSet::new();
    for relative_path in removed_relative_paths {
        relative_paths_to_remove.insert(relative_path);
    }
    if replace_all {
        for relative_path in existing_by_path.keys() {
            if !seen_relative_paths.contains(relative_path) {
                relative_paths_to_remove.insert(relative_path.clone());
            }
        }
    }

    for relative_path in relative_paths_to_remove {
        if mark_document_removed(&transaction, &relative_path)? {
            removed_document_count += 1;
        }
    }

    transaction.commit()?;
    let (active_document_count, active_chunk_count) = count_active_cache_rows(&connection)?;
    Ok(IncrementalBuildStats {
        scanned_document_count,
        new_document_count,
        changed_document_count,
        unchanged_document_count,
        removed_document_count,
        active_document_count,
        active_chunk_count,
    })
}

pub fn export_artifact_from_build_cache(
    cache_path: &Path,
    output_path: &Path,
) -> Result<BuildStats> {
    let cache = Connection::open(cache_path)?;
    let info = load_build_cache_info(&cache)?;
    let active_documents = load_active_documents(&cache)?;
    let active_chunks = load_active_chunks(&cache)?;

    if output_path.exists() {
        fs::remove_file(output_path)?;
    }
    let mut artifact = Connection::open(output_path)?;
    initialize_artifact_schema(&artifact)?;

    let built_at = now_as_string()?;
    artifact.execute(
        "INSERT INTO artifact_meta (key, value) VALUES (?1, ?2)",
        params!["schema_version", "2"],
    )?;
    artifact.execute(
        "INSERT INTO artifact_meta (key, value) VALUES (?1, ?2)",
        params!["built_at", built_at],
    )?;
    artifact.execute(
        "INSERT INTO artifact_meta (key, value) VALUES (?1, ?2)",
        params!["source_root", serde_json::to_string(&info.source_root)?],
    )?;
    artifact.execute(
        "INSERT INTO artifact_meta (key, value) VALUES (?1, ?2)",
        params![
            "embedding_backend",
            serde_json::to_string(&info.embedding_backend)?
        ],
    )?;
    artifact.execute(
        "INSERT INTO artifact_meta (key, value) VALUES (?1, ?2)",
        params!["lexical_tokenizer", info.lexical_tokenizer],
    )?;
    artifact.execute(
        "INSERT INTO artifact_meta (key, value) VALUES (?1, ?2)",
        params![
            "chunking",
            serde_json::to_string(&json!({
                "target_tokens": info.chunking.target_tokens,
                "overlap_tokens": info.chunking.overlap_tokens,
            }))?
        ],
    )?;

    let transaction = artifact.transaction()?;
    for document in &active_documents {
        transaction.execute(
            "INSERT INTO documents (
                doc_id, source_root_id, source_path, relative_path, canonical_url, title,
                summary, content_hash, modified_at, chunk_count, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                document.doc_id,
                document.source_root_id,
                document.source_path,
                document.relative_path,
                document.canonical_url,
                document.title,
                document.summary,
                document.content_hash,
                document.modified_at,
                document.chunk_count as i64,
                serde_json::to_string(&document.metadata)?,
            ],
        )?;
    }
    for chunk in &active_chunks {
        transaction.execute(
            "INSERT INTO chunks (
                chunk_id, doc_id, ordinal, heading_path_json, char_start, char_end,
                token_count, chunk_text, excerpt
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                chunk.chunk_id,
                chunk.doc_id,
                chunk.ordinal as i64,
                serde_json::to_string(&chunk.heading_path)?,
                chunk.char_start as i64,
                chunk.char_end as i64,
                chunk.token_count as i64,
                chunk.chunk_text,
                chunk.excerpt,
            ],
        )?;
        transaction.execute(
            "INSERT INTO chunk_vectors (chunk_id, dimensions, vector_blob) VALUES (?1, ?2, ?3)",
            params![chunk.chunk_id, chunk.dimensions as i64, chunk.vector_blob],
        )?;
        transaction.execute(
            "INSERT INTO fts_chunks (chunk_id, doc_id, chunk_text, excerpt) VALUES (?1, ?2, ?3, ?4)",
            params![
                chunk.chunk_id,
                chunk.doc_id,
                chunk.lexical_text,
                chunk.lexical_excerpt,
            ],
        )?;
    }
    transaction.commit()?;

    Ok(BuildStats {
        document_count: active_documents.len(),
        chunk_count: active_chunks.len(),
    })
}

pub fn export_canonical_from_build_cache(
    cache_path: &Path,
    output_dir: &Path,
) -> Result<CanonicalBuildStats> {
    let cache = Connection::open(cache_path)?;
    let info = load_build_cache_info(&cache)?;
    let active_documents = load_active_documents(&cache)?;
    let active_chunks = load_active_chunks(&cache)?;

    fs::create_dir_all(output_dir)?;
    let mut canonical_documents = Vec::with_capacity(active_documents.len());
    let mut canonical_chunks = Vec::with_capacity(active_chunks.len());
    let mut vectors = Vec::new();

    let mut first_chunk_indices = BTreeMap::new();
    let mut chunk_counts = BTreeMap::new();
    for (chunk_index, chunk) in active_chunks.iter().enumerate() {
        first_chunk_indices
            .entry(chunk.doc_id.clone())
            .or_insert(chunk_index);
        *chunk_counts.entry(chunk.doc_id.clone()).or_insert(0usize) += 1;
        canonical_chunks.push(CanonicalChunkRecord {
            chunk_id: chunk.chunk_id,
            doc_id: chunk.doc_id.clone(),
            ordinal: chunk.ordinal,
            heading_path: chunk.heading_path.clone(),
            char_start: chunk.char_start,
            char_end: chunk.char_end,
            token_count: chunk.token_count,
            excerpt: chunk.excerpt.clone(),
            chunk_text: chunk.chunk_text.clone(),
        });
        vectors.extend_from_slice(&chunk.vector_blob);
    }

    for document in &active_documents {
        canonical_documents.push(CanonicalDocumentRecord {
            doc_id: document.doc_id.clone(),
            relative_path: document.relative_path.clone(),
            canonical_url: document.canonical_url.clone(),
            title: document.title.clone(),
            summary: document.summary.clone(),
            metadata: document.metadata.clone(),
            first_chunk_index: first_chunk_indices
                .get(&document.doc_id)
                .copied()
                .unwrap_or(0),
            chunk_count: chunk_counts.get(&document.doc_id).copied().unwrap_or(0),
        });
    }

    let postings = build_postings(&canonical_chunks);
    let vector_dimensions = active_chunks
        .first()
        .map(|chunk| chunk.dimensions)
        .unwrap_or(0);
    let model_files = maybe_write_model_assets(output_dir, &info.embedding_backend)?;
    let mut features = vec![
        "vector-search".to_string(),
        "lexical-postings".to_string(),
        "retrieval-only-results".to_string(),
    ];
    if model_files.is_some() {
        features.push("model2vec-query".to_string());
    }

    let manifest = CanonicalArtifactManifest {
        schema_version: "1".to_string(),
        artifact_format: "file-bundle-v1".to_string(),
        built_at: now_as_string()?,
        embedding_backend: info.embedding_backend.clone(),
        document_count: canonical_documents.len(),
        chunk_count: canonical_chunks.len(),
        vector_dimensions,
        chunking: CanonicalChunkingConfig {
            target_tokens: info.chunking.target_tokens,
            overlap_tokens: info.chunking.overlap_tokens,
        },
        files: CanonicalArtifactFiles {
            documents: "documents.json".to_string(),
            chunks: "chunks.json".to_string(),
            vectors: "vectors.bin".to_string(),
            postings: "postings.json".to_string(),
            model: model_files,
        },
        features,
    };

    fs::write(
        output_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        output_dir.join("documents.json"),
        serde_json::to_vec_pretty(&canonical_documents)?,
    )?;
    fs::write(
        output_dir.join("chunks.json"),
        serde_json::to_vec_pretty(&canonical_chunks)?,
    )?;
    fs::write(output_dir.join("vectors.bin"), vectors)?;
    fs::write(
        output_dir.join("postings.json"),
        serde_json::to_vec_pretty(&postings)?,
    )?;

    Ok(CanonicalBuildStats {
        document_count: manifest.document_count,
        chunk_count: manifest.chunk_count,
        vector_dimensions,
    })
}

pub fn load_build_cache_info(connection: &Connection) -> Result<BuildCacheInfo> {
    let meta = load_cache_meta(connection)?;
    let source_root = serde_json::from_str(
        meta.get("source_root")
            .ok_or(IndexbindError::MissingMetadata("source_root"))?,
    )?;
    let embedding_backend = serde_json::from_str(
        meta.get("embedding_backend")
            .ok_or(IndexbindError::MissingMetadata("embedding_backend"))?,
    )?;
    let chunking_value: Value = serde_json::from_str(
        meta.get("chunking")
            .ok_or(IndexbindError::MissingMetadata("chunking"))?,
    )?;
    let (document_count, chunk_count) = count_active_cache_rows(connection)?;

    Ok(BuildCacheInfo {
        schema_version: meta
            .get("schema_version")
            .cloned()
            .ok_or(IndexbindError::MissingMetadata("schema_version"))?,
        source_root,
        embedding_backend,
        lexical_tokenizer: meta
            .get("lexical_tokenizer")
            .cloned()
            .ok_or(IndexbindError::MissingMetadata("lexical_tokenizer"))?,
        chunking: CacheChunkingInfo {
            target_tokens: chunking_value
                .get("target_tokens")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize,
            overlap_tokens: chunking_value
                .get("overlap_tokens")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize,
        },
        document_count,
        chunk_count,
    })
}

fn initialize_cache_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS cache_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS documents (
            doc_id TEXT PRIMARY KEY,
            source_root_id TEXT NOT NULL,
            source_path TEXT,
            relative_path TEXT NOT NULL UNIQUE,
            canonical_url TEXT,
            title TEXT,
            summary TEXT,
            content_hash TEXT NOT NULL,
            document_fingerprint TEXT NOT NULL,
            modified_at INTEGER,
            chunk_count INTEGER NOT NULL,
            metadata_json TEXT NOT NULL,
            is_deleted INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS chunks (
            chunk_id INTEGER PRIMARY KEY,
            doc_id TEXT NOT NULL,
            ordinal INTEGER NOT NULL,
            heading_path_json TEXT NOT NULL,
            char_start INTEGER NOT NULL,
            char_end INTEGER NOT NULL,
            token_count INTEGER NOT NULL,
            chunk_text TEXT NOT NULL,
            excerpt TEXT NOT NULL,
            lexical_text TEXT NOT NULL,
            lexical_excerpt TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS chunk_vectors (
            chunk_id INTEGER PRIMARY KEY,
            dimensions INTEGER NOT NULL,
            vector_blob BLOB NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_documents_relative_path ON documents(relative_path);
        CREATE INDEX IF NOT EXISTS idx_chunks_doc_id ON chunks(doc_id);
        ",
    )?;
    Ok(())
}

fn refresh_cache_meta(connection: &Connection, options: &BuildArtifactOptions) -> Result<bool> {
    let desired = BTreeMap::from([
        (
            "schema_version".to_string(),
            CACHE_SCHEMA_VERSION.to_string(),
        ),
        (
            "source_root".to_string(),
            serde_json::to_string(&options.source_root)?,
        ),
        (
            "embedding_backend".to_string(),
            serde_json::to_string(&options.embedding_backend)?,
        ),
        (
            "embedding_backend_fingerprint".to_string(),
            build_embedding_backend_fingerprint(&options.embedding_backend),
        ),
        (
            "chunking".to_string(),
            serde_json::to_string(&json!({
                "target_tokens": options.chunking.target_tokens,
                "overlap_tokens": options.chunking.overlap_tokens,
            }))?,
        ),
        (
            "chunking_fingerprint".to_string(),
            build_chunking_fingerprint(&options.chunking),
        ),
        (
            "lexical_tokenizer".to_string(),
            LEXICAL_TOKENIZER_VERSION.to_string(),
        ),
    ]);
    let existing = load_cache_meta(connection)?;
    let config_changed = existing
        .get("source_root")
        .is_some_and(|value| value != desired.get("source_root").unwrap())
        || existing
            .get("embedding_backend_fingerprint")
            .is_some_and(|value| value != desired.get("embedding_backend_fingerprint").unwrap())
        || existing
            .get("chunking_fingerprint")
            .is_some_and(|value| value != desired.get("chunking_fingerprint").unwrap())
        || existing
            .get("lexical_tokenizer")
            .is_some_and(|value| value != desired.get("lexical_tokenizer").unwrap());

    for (key, value) in desired {
        connection.execute(
            "INSERT INTO cache_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
    }
    Ok(config_changed)
}

fn load_cache_meta(connection: &Connection) -> Result<BTreeMap<String, String>> {
    let mut statement = connection.prepare("SELECT key, value FROM cache_meta")?;
    let rows = statement.query_map([], |row| {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        Ok((key, value))
    })?;
    let mut meta = BTreeMap::new();
    for row in rows {
        let (key, value) = row?;
        meta.insert(key, value);
    }
    Ok(meta)
}

fn clear_cache_contents(connection: &Connection) -> Result<()> {
    connection.execute("DELETE FROM chunk_vectors", [])?;
    connection.execute("DELETE FROM chunks", [])?;
    connection.execute("DELETE FROM documents", [])?;
    Ok(())
}

fn load_cached_document_fingerprints(
    connection: &Connection,
) -> Result<Vec<CachedDocumentFingerprint>> {
    let mut statement = connection
        .prepare("SELECT relative_path, document_fingerprint, is_deleted FROM documents")?;
    let rows = statement.query_map([], |row| {
        Ok(CachedDocumentFingerprint {
            relative_path: row.get(0)?,
            document_fingerprint: row.get(1)?,
            is_deleted: row.get::<_, i64>(2)? != 0,
        })
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

fn materialize_document(
    document: &NormalizedDocument,
    options: &BuildArtifactOptions,
    embedder: &mut Embedder,
) -> Result<MaterializedDocument> {
    let doc_id = document
        .doc_id
        .clone()
        .unwrap_or_else(|| build_doc_id(&options.source_root.id, &document.relative_path));
    let content_hash = build_content_hash(&document.content);
    let document_fingerprint = build_document_fingerprint(document);
    let mut chunks = chunk_document(&doc_id, &document.content, &options.chunking);
    for chunk in &mut chunks {
        chunk.chunk_id = build_chunk_id(&doc_id, chunk.ordinal);
    }
    let embedding_inputs = chunks
        .iter()
        .map(|chunk| {
            format_chunk_for_embedding(
                &document.relative_path,
                document.title.as_deref(),
                &chunk.heading_path,
                &chunk.chunk_text,
            )
        })
        .collect::<Vec<_>>();
    let embeddings = embedder.embed_texts(&embedding_inputs)?;

    let materialized_chunks = chunks
        .into_iter()
        .zip(embeddings.into_iter())
        .map(|(chunk, embedding)| MaterializedChunk {
            lexical_text: tokenize_for_storage(&chunk.chunk_text),
            lexical_excerpt: tokenize_for_storage(&chunk.excerpt),
            dimensions: embedding.len(),
            vector_blob: vector_to_bytes(&embedding),
            stored_chunk: chunk,
        })
        .collect::<Vec<_>>();

    Ok(MaterializedDocument {
        stored_document: StoredDocument {
            doc_id,
            source_root_id: options.source_root.id.clone(),
            source_path: document.source_path.clone(),
            relative_path: document.relative_path.clone(),
            canonical_url: document.canonical_url.clone(),
            title: document.title.clone(),
            summary: document.summary.clone(),
            content_hash,
            modified_at: None,
            chunk_count: materialized_chunks.len(),
            metadata: document.metadata.clone(),
        },
        document_fingerprint,
        chunks: materialized_chunks,
    })
}

fn upsert_materialized_document(
    connection: &Connection,
    document: &MaterializedDocument,
    source_root_id: &str,
) -> Result<()> {
    connection.execute(
        "INSERT INTO documents (
            doc_id, source_root_id, source_path, relative_path, canonical_url, title,
            summary, content_hash, document_fingerprint, modified_at, chunk_count,
            metadata_json, is_deleted
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, 0)
        ON CONFLICT(doc_id) DO UPDATE SET
            source_root_id=excluded.source_root_id,
            source_path=excluded.source_path,
            relative_path=excluded.relative_path,
            canonical_url=excluded.canonical_url,
            title=excluded.title,
            summary=excluded.summary,
            content_hash=excluded.content_hash,
            document_fingerprint=excluded.document_fingerprint,
            modified_at=excluded.modified_at,
            chunk_count=excluded.chunk_count,
            metadata_json=excluded.metadata_json,
            is_deleted=0",
        params![
            document.stored_document.doc_id,
            source_root_id,
            document.stored_document.source_path,
            document.stored_document.relative_path,
            document.stored_document.canonical_url,
            document.stored_document.title,
            document.stored_document.summary,
            document.stored_document.content_hash,
            document.document_fingerprint,
            document.stored_document.chunk_count as i64,
            serde_json::to_string(&document.stored_document.metadata)?,
        ],
    )?;

    connection.execute(
        "DELETE FROM chunk_vectors WHERE chunk_id IN (SELECT chunk_id FROM chunks WHERE doc_id = ?1)",
        params![document.stored_document.doc_id],
    )?;
    connection.execute(
        "DELETE FROM chunks WHERE doc_id = ?1",
        params![document.stored_document.doc_id],
    )?;

    for chunk in &document.chunks {
        connection.execute(
            "INSERT INTO chunks (
                chunk_id, doc_id, ordinal, heading_path_json, char_start, char_end,
                token_count, chunk_text, excerpt, lexical_text, lexical_excerpt
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                chunk.stored_chunk.chunk_id,
                chunk.stored_chunk.doc_id,
                chunk.stored_chunk.ordinal as i64,
                serde_json::to_string(&chunk.stored_chunk.heading_path)?,
                chunk.stored_chunk.char_start as i64,
                chunk.stored_chunk.char_end as i64,
                chunk.stored_chunk.token_count as i64,
                chunk.stored_chunk.chunk_text,
                chunk.stored_chunk.excerpt,
                chunk.lexical_text,
                chunk.lexical_excerpt,
            ],
        )?;
        connection.execute(
            "INSERT INTO chunk_vectors (chunk_id, dimensions, vector_blob) VALUES (?1, ?2, ?3)",
            params![
                chunk.stored_chunk.chunk_id,
                chunk.dimensions as i64,
                chunk.vector_blob,
            ],
        )?;
    }
    Ok(())
}

fn mark_document_removed(connection: &Connection, relative_path: &str) -> Result<bool> {
    let doc_id: Option<String> = connection
        .query_row(
            "SELECT doc_id FROM documents WHERE relative_path = ?1 AND is_deleted = 0",
            params![relative_path],
            |row| row.get(0),
        )
        .optional()?;
    let Some(doc_id) = doc_id else {
        return Ok(false);
    };
    connection.execute(
        "UPDATE documents SET is_deleted = 1 WHERE doc_id = ?1",
        params![doc_id],
    )?;
    connection.execute(
        "DELETE FROM chunk_vectors WHERE chunk_id IN (SELECT chunk_id FROM chunks WHERE doc_id = ?1)",
        params![doc_id],
    )?;
    connection.execute("DELETE FROM chunks WHERE doc_id = ?1", params![doc_id])?;
    Ok(true)
}

fn count_active_cache_rows(connection: &Connection) -> Result<(usize, usize)> {
    let document_count = connection.query_row(
        "SELECT COUNT(*) FROM documents WHERE is_deleted = 0",
        [],
        |row| row.get::<_, i64>(0),
    )? as usize;
    let chunk_count = connection.query_row("SELECT COUNT(*) FROM chunks", [], |row| {
        row.get::<_, i64>(0)
    })? as usize;
    Ok((document_count, chunk_count))
}

fn load_active_documents(connection: &Connection) -> Result<Vec<StoredDocument>> {
    let mut statement = connection.prepare(
        "SELECT doc_id, source_root_id, source_path, relative_path, canonical_url, title,
                summary, content_hash, modified_at, chunk_count, metadata_json
         FROM documents
         WHERE is_deleted = 0
         ORDER BY relative_path ASC",
    )?;
    let rows = statement.query_map([], |row| {
        let metadata_json: String = row.get(10)?;
        Ok(StoredDocument {
            doc_id: row.get(0)?,
            source_root_id: row.get(1)?,
            source_path: row.get(2)?,
            relative_path: row.get(3)?,
            canonical_url: row.get(4)?,
            title: row.get(5)?,
            summary: row.get(6)?,
            content_hash: row.get(7)?,
            modified_at: row.get(8)?,
            chunk_count: row.get::<_, i64>(9)? as usize,
            metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
        })
    })?;
    let mut documents = Vec::new();
    for row in rows {
        documents.push(row?);
    }
    Ok(documents)
}

fn load_active_chunks(connection: &Connection) -> Result<Vec<LoadedCachedChunk>> {
    let mut statement = connection.prepare(
        "SELECT c.chunk_id, c.doc_id, c.ordinal, c.heading_path_json, c.char_start, c.char_end,
                c.token_count, c.chunk_text, c.excerpt, c.lexical_text, c.lexical_excerpt,
                v.dimensions, v.vector_blob
         FROM chunks c
         INNER JOIN documents d ON d.doc_id = c.doc_id
         INNER JOIN chunk_vectors v ON v.chunk_id = c.chunk_id
         WHERE d.is_deleted = 0
         ORDER BY d.relative_path ASC, c.ordinal ASC",
    )?;
    let rows = statement.query_map([], |row| {
        let heading_path_json: String = row.get(3)?;
        Ok(LoadedCachedChunk {
            chunk_id: row.get(0)?,
            doc_id: row.get(1)?,
            ordinal: row.get::<_, i64>(2)? as usize,
            heading_path: serde_json::from_str(&heading_path_json).unwrap_or_default(),
            char_start: row.get::<_, i64>(4)? as usize,
            char_end: row.get::<_, i64>(5)? as usize,
            token_count: row.get::<_, i64>(6)? as usize,
            chunk_text: row.get(7)?,
            excerpt: row.get(8)?,
            lexical_text: row.get(9)?,
            lexical_excerpt: row.get(10)?,
            dimensions: row.get::<_, i64>(11)? as usize,
            vector_blob: row.get(12)?,
        })
    })?;
    let mut chunks = Vec::new();
    for row in rows {
        chunks.push(row?);
    }
    Ok(chunks)
}

#[derive(Debug)]
struct LoadedCachedChunk {
    chunk_id: i64,
    doc_id: String,
    ordinal: usize,
    heading_path: Vec<String>,
    char_start: usize,
    char_end: usize,
    token_count: usize,
    chunk_text: String,
    excerpt: String,
    lexical_text: String,
    lexical_excerpt: String,
    dimensions: usize,
    vector_blob: Vec<u8>,
}

fn now_as_string() -> Result<String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| IndexbindError::Embedding(error.into()))?
        .as_secs()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        export_artifact_from_build_cache, export_canonical_from_build_cache, load_build_cache_info,
        update_build_cache, BuildCacheUpdate,
    };
    use crate::build::BuildArtifactOptions;
    use crate::embedding::EmbeddingBackend;
    use crate::{NormalizedDocument, Retriever, SearchOptions};
    use rusqlite::Connection;
    use serde_json::json;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    #[test]
    fn reuses_unchanged_documents_and_rebuilds_changed_ones() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("build-cache.sqlite");
        let options = BuildArtifactOptions {
            embedding_backend: EmbeddingBackend::Hashing { dimensions: 16 },
            ..Default::default()
        };

        let first = update_build_cache(
            &cache,
            BuildCacheUpdate {
                documents: vec![
                    document("docs/a.md", "Alpha"),
                    document("docs/b.md", "Beta"),
                ],
                removed_relative_paths: Vec::new(),
                replace_all: true,
            },
            &options,
        )
        .unwrap();
        assert_eq!(first.new_document_count, 2);
        assert_eq!(first.active_document_count, 2);

        let second = update_build_cache(
            &cache,
            BuildCacheUpdate {
                documents: vec![
                    document("docs/a.md", "Alpha updated"),
                    document("docs/b.md", "Beta"),
                ],
                removed_relative_paths: Vec::new(),
                replace_all: true,
            },
            &options,
        )
        .unwrap();
        assert_eq!(second.changed_document_count, 1);
        assert_eq!(second.unchanged_document_count, 1);
        assert_eq!(second.removed_document_count, 0);
    }

    #[test]
    fn partial_updates_can_remove_documents_without_full_replace() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("build-cache.sqlite");
        let options = BuildArtifactOptions {
            embedding_backend: EmbeddingBackend::Hashing { dimensions: 16 },
            ..Default::default()
        };

        update_build_cache(
            &cache,
            BuildCacheUpdate {
                documents: vec![
                    document("docs/a.md", "Alpha"),
                    document("docs/b.md", "Beta"),
                ],
                removed_relative_paths: Vec::new(),
                replace_all: true,
            },
            &options,
        )
        .unwrap();

        let stats = update_build_cache(
            &cache,
            BuildCacheUpdate {
                documents: vec![document("docs/a.md", "Alpha 2")],
                removed_relative_paths: vec!["docs/b.md".to_string()],
                replace_all: false,
            },
            &options,
        )
        .unwrap();
        assert_eq!(stats.changed_document_count, 1);
        assert_eq!(stats.removed_document_count, 1);
        assert_eq!(stats.active_document_count, 1);
    }

    #[test]
    fn exports_artifact_and_bundle_from_cache() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("build-cache.sqlite");
        let artifact = dir.path().join("index.sqlite");
        let bundle = dir.path().join("bundle");
        let options = BuildArtifactOptions {
            embedding_backend: EmbeddingBackend::Hashing { dimensions: 16 },
            ..Default::default()
        };

        update_build_cache(
            &cache,
            BuildCacheUpdate {
                documents: vec![document("guides/rust.md", "Rust retrieval guide")],
                removed_relative_paths: Vec::new(),
                replace_all: true,
            },
            &options,
        )
        .unwrap();

        let artifact_stats = export_artifact_from_build_cache(&cache, &artifact).unwrap();
        assert_eq!(artifact_stats.document_count, 1);
        let mut retriever = Retriever::open(&artifact).unwrap();
        let hits = retriever
            .search("Rust retrieval", SearchOptions::default())
            .unwrap();
        assert_eq!(hits[0].relative_path, "guides/rust.md");

        let bundle_stats = export_canonical_from_build_cache(&cache, &bundle).unwrap();
        assert_eq!(bundle_stats.document_count, 1);
        assert!(bundle.join("manifest.json").exists());
        assert!(bundle.join("postings.json").exists());
    }

    #[test]
    fn stores_cache_metadata_for_export() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("build-cache.sqlite");
        let options = BuildArtifactOptions {
            embedding_backend: EmbeddingBackend::Hashing { dimensions: 8 },
            ..Default::default()
        };
        update_build_cache(
            &cache,
            BuildCacheUpdate {
                documents: vec![document("docs/a.md", "Alpha")],
                removed_relative_paths: Vec::new(),
                replace_all: true,
            },
            &options,
        )
        .unwrap();

        let connection = Connection::open(&cache).unwrap();
        let info = load_build_cache_info(&connection).unwrap();
        assert_eq!(info.document_count, 1);
        assert_eq!(info.chunking.target_tokens, options.chunking.target_tokens);
        assert_eq!(info.lexical_tokenizer, crate::LEXICAL_TOKENIZER_VERSION);
    }

    fn document(relative_path: &str, content: &str) -> NormalizedDocument {
        let mut metadata = BTreeMap::new();
        metadata.insert("lang".to_string(), json!("rust"));
        NormalizedDocument {
            doc_id: None,
            source_path: Some(relative_path.to_string()),
            relative_path: relative_path.to_string(),
            canonical_url: None,
            title: Some(relative_path.to_string()),
            summary: None,
            content: content.to_string(),
            metadata,
        }
    }
}
