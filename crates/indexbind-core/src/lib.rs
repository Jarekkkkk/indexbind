#[cfg(not(target_arch = "wasm32"))]
mod artifact;
mod build;
mod canonical;
mod chunking;
mod embedding;
mod error;
mod lexical;
#[cfg(not(target_arch = "wasm32"))]
mod retriever;
mod types;

#[cfg(not(target_arch = "wasm32"))]
pub use artifact::build_artifact;
pub use build::{BuildArtifactOptions, BuildStats};
pub use canonical::{
    build_canonical_artifact, CanonicalArtifactManifest, CanonicalBuildStats, CanonicalChunkRecord,
    CanonicalDocumentRecord, CanonicalPosting, CanonicalPostings,
};
pub use chunking::ChunkingOptions;
pub use embedding::EmbeddingBackend;
pub use error::{IndexbindError, Result};
pub use lexical::{
    estimate_token_count, normalize_for_heuristic, tokenize as lexical_tokenize,
    tokenize_for_storage as lexical_tokenize_for_storage, LEXICAL_TOKENIZER_VERSION,
};
#[cfg(not(target_arch = "wasm32"))]
pub use retriever::{ArtifactInfo, RerankerKind, RerankerOptions, Retriever, SearchOptions};
pub use types::{
    BestMatch, DocumentHit, MetadataMap, NormalizedDocument, SourceRoot, StoredChunk,
    StoredDocument,
};
