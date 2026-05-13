#[cfg(not(target_arch = "wasm32"))]
mod artifact;
mod build;
#[cfg(not(target_arch = "wasm32"))]
mod build_cache;
mod canonical;
mod chunking;
#[cfg(not(target_arch = "wasm32"))]
mod cross_encoder;
mod embedding;
mod error;
mod lexical;
#[cfg(not(target_arch = "wasm32"))]
mod retriever;
mod types;

#[cfg(not(target_arch = "wasm32"))]
pub use artifact::build_artifact;
pub use build::{BuildArtifactOptions, BuildStats, IncrementalBuildStats};
#[cfg(not(target_arch = "wasm32"))]
pub use build_cache::{
    export_artifact_from_build_cache, export_canonical_from_build_cache, load_build_cache_info,
    update_build_cache, BuildCacheInfo, BuildCacheUpdate, CacheChunkingInfo,
};
pub use canonical::{
    build_canonical_artifact, CanonicalArtifactManifest, CanonicalBuildStats, CanonicalChunkRecord,
    CanonicalDocumentRecord, CanonicalPosting, CanonicalPostings,
};
pub use chunking::{chunk_document, ChunkingOptions};
pub use embedding::{format_chunk_for_embedding, Embedder, EmbeddingBackend};
pub use error::{IndexbindError, Result};
pub use lexical::{
    estimate_token_count, normalize_for_heuristic, tokenize as lexical_tokenize,
    tokenize_for_storage as lexical_tokenize_for_storage, LEXICAL_TOKENIZER_VERSION,
};
#[cfg(not(target_arch = "wasm32"))]
pub use retriever::{
    ArtifactInfo, RetrievalMode, RerankerKind, RerankerOptions, Retriever,
    RetrieverOpenOptions, ScoreAdjustmentOptions, SearchOptions, ModeProfile,
};
#[cfg(not(target_arch = "wasm32"))]
pub use cross_encoder::CrossEncoder;
pub use types::{
    BestMatch, ChunkHit, DocumentHit, MetadataMap, NormalizedDocument, SourceRoot, StoredChunk,
    StoredDocument,
};
