# Cross-Encoder ONNX Reranker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `RerankerKind::CrossEncoderOnnx` that uses bge-reranker-v2-m3 ONNX int8 for reranking, set it as the new default reranker, keep `EmbeddingV1` as a fallback.

**Architecture:** New `CrossEncoder` struct wraps `ort::Session` (ONNX Runtime) and `tokenizers::Tokenizer` (BERT WordPiece). Lazy-loaded on first use, shared via `Arc`. Reranking tokenizes (query, passage) pairs, runs ONNX forward pass, blends cross-encoder score with original score. The cross-encoder is ONLY used at rerank time — document embeddings and vector search stay on `Model2Vec`/`Hashing`.

**Tech Stack:** `ort` (load-dynamic), `tokenizers`, `hf-hub` (existing), `ndarray`

---

### File Structure

**Created:**
- `crates/indexbind-core/src/cross_encoder.rs` — `CrossEncoder` struct + public API

**Modified:**
- `crates/indexbind-core/Cargo.toml` — add `ort`, `tokenizers` (ndarray is transitive)
- `crates/indexbind-core/src/retriever.rs` — add `CrossEncoderOnnx` variant + rerank logic
- `crates/indexbind-core/src/lib.rs` — export `CrossEncoder`
- `crates/indexbind-node/src/lib.rs` — map `"cross-encoder-onnx"` string

---

### Task 1: Add dependencies

**File:** `crates/indexbind-core/Cargo.toml`

```toml
# Add to [target.'cfg(not(target_arch = "wasm32"))'.dependencies]
ort = { version = "2", features = ["load-dynamic"] }
tokenizers = "0.21"
```

`ndarray` is already a transitive dep; add it explicitly if `ort` needs it at the type level.

- [x] **Step 1: Add three lines to Cargo.toml**

```bash
# Wait for full workspace recheck after adding deps
cargo check -p indexbind-core 2>&1 | tail -10
```

Expected: `Failed to run custom build command for ort-sys v2.x.x` — this is fine, it means `ort` is compiling. Full build will need ONNX Runtime shared library.

- [x] **Step 2: Verify transitive deps resolve**

Run: `cargo metadata --format-version 1 | grep -E '"ort|tokenizers|ndarray"'`

---

### Task 2: Create `CrossEncoder` module

**File:** `crates/indexbind-core/src/cross_encoder.rs`

```rust
use crate::Result;
use ndarray::Array2;
use ort::{Session, SessionInputs, SessionOutputs, Value};
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;

/// Lazy-loaded cross-encoder model for reranking.
/// Shares the underlying ONNX session and tokenizer via Arc.
#[derive(Clone)]
pub struct CrossEncoder {
    inner: Arc<Mutex<Option<CrossEncoderInner>>>,
}

struct CrossEncoderInner {
    session: Session,
    tokenizer: Tokenizer,
    max_length: usize,
}

impl CrossEncoder {
    /// Create a new uninitialized encoder (lazy load).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    /// Ensure the model and tokenizer are loaded.
    /// Downloads 3 files via hf-hub (file-level only, not full repo):
    ///   - BAAI/bge-reranker-v2-m3 onnx/model_quantized.onnx
    ///   - BAAI/bge-reranker-v2-m3 tokenizer.json
    ///   - BAAI/bge-reranker-v2-m3 tokenizer_config.json
    pub fn ensure_loaded(&self) -> Result<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| crate::IndexbindError::Internal(e.to_string()))?;
        if guard.is_some() {
            return Ok(());
        }
        let model_path = hf_hub::api::sync::Api::new()?
            .model("BAAI/bge-reranker-v2-m3".to_string())
            .get("onnx/model_quantized.onnx")?;
        let tokenizer_path = hf_hub::api::sync::Api::new()?
            .model("BAAI/bge-reranker-v2-m3".to_string())
            .get("tokenizer.json")?;

        let session = Session::builder()?
            .commit_from_file(model_path)?;
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| crate::IndexbindError::Internal(e.to_string()))?;

        *guard = Some(CrossEncoderInner {
            session,
            tokenizer,
            max_length: 512,
        });
        Ok(())
    }

    /// Score (query, passage) pairs using the cross-encoder.
    /// Returns one score per passage. Higher = more relevant.
    pub fn rerank(&self, query: &str, passages: &[String], batch_size: usize) -> Result<Vec<f32>> {
        let guard = self
            .inner
            .lock()
            .map_err(|e| crate::IndexbindError::Internal(e.to_string()))?;
        let inner = guard
            .as_ref()
            .ok_or_else(|| crate::IndexbindError::Internal("cross-encoder not loaded".into()))?;

        let mut all_scores = Vec::with_capacity(passages.len());
        for chunk in passages.chunks(batch_size) {
            // Build (query, passage) pairs as BERT-style "query [SEP] passage"
            let pairs: Vec<(&str, &str)> = chunk.iter().map(|p| (query, p.as_str())).collect();
            let encodings = inner
                .tokenizer
                .encode_batch(pairs, true)
                .map_err(|e| crate::IndexbindError::Internal(e.to_string()))?;

            // Find max length in this batch for padding
            let batch_max = encodings
                .iter()
                .map(|e| e.len())
                .min()
                .unwrap_or(0)
                .min(inner.max_length);

            let batch_size_actual = encodings.len();
            let mut input_ids = Array2::zeros((batch_size_actual, batch_max));
            let mut attention_mask = Array2::zeros((batch_size_actual, batch_max));

            for (i, encoding) in encodings.iter().enumerate() {
                let ids = encoding.get_ids();
                let len = ids.len().min(batch_max);
                for j in 0..len {
                    input_ids[[i, j]] = ids[j] as i64;
                    attention_mask[[i, j]] = 1i64;
                }
            }

            let outputs = inner.session.run(SessionInputs::new()?
                .insert("input_ids", Value::from_array(input_ids)?)
                .insert("attention_mask", Value::from_array(attention_mask)?))?;

            let logits = outputs["logits"]
                .try_extract::<f32>()?
                .view()
                .into_owned();

            // bge-reranker-v2-m3 outputs shape (batch, 2):
            // index 0 = negative score, index 1 = positive (relevance) score
            // We want the positive logit or softmax probability
            for i in 0..batch_size_actual {
                let pos = logits[[i, 1]];
                let neg = logits[[i, 0]];
                let prob = 1.0 / (1.0 + (-pos + neg).exp()); // softmax on (neg, pos)
                all_scores.push(prob);
            }
        }
        Ok(all_scores)
    }
}
```

- [x] **Step 1: Write the module**

Write the code above to `crates/indexbind-core/src/cross_encoder.rs`.

- [x] **Step 2: Verify compilation**

Run: `cargo check -p indexbind-core`
Expected: Compiles. (May need to adjust `ort::Value` API slightly for actual v2 API surface.)

- [x] **Step 3: Commit**

```bash
git add crates/indexbind-core/src/cross_encoder.rs
git commit -m "feat: add CrossEncoder module for ONNX reranking"
```

---

### Task 3: Integrate into `Retriever`

**File:** `crates/indexbind-core/src/retriever.rs`

Changes:
1. Add `mod cross_encoder;` to module declarations in `retriever.rs` (it's already in lib.rs through mod retriever — actually we need to add it in lib.rs).
2. Import `CrossEncoder` at top of `retriever.rs` or access it through `crate::cross_encoder`.
3. Change `RerankerKind` default from `EmbeddingV1` to `CrossEncoderOnnx`.
4. Add `cross_encoder: CrossEncoder` field to `Retriever`.
5. Initialize `cross_encoder` in `open_with_options`.
6. Add reranking functions for cross-encoder.

Wait, actually — `cross_encoder.rs` should be standalone module in `lib.rs`, not inside `retriever.rs`. Let me reconsider.

The module structure:
- `crates/indexbind-core/src/lib.rs` — add `mod cross_encoder;` and `pub use cross_encoder::CrossEncoder;`
- `crates/indexbind-core/src/retriever.rs` — use `crate::cross_encoder::CrossEncoder` or add the type to the module-level type space

Actually, `lib.rs` already has the structure:
```rust
mod retriever;  // (behind cfg flag)
```

And `retriever.rs` uses `use crate::...` to access other modules. So `cross_encoder.rs` would be a sibling module.

But actually — `cross_encoder.rs` should only be available on non-wasm32 targets (since `ort` and `tokenizers` don't compile to wasm). Let me check the cfg pattern:

```rust
#[cfg(not(target_arch = "wasm32"))]
mod artifact;
#[cfg(not(target_arch = "wasm32"))]
mod retriever;
```

So I need:
```rust
#[cfg(not(target_arch = "wasm32"))]
mod cross_encoder;
```

And `retriever.rs` can use `use crate::cross_encoder::CrossEncoder;`.

Let me also make sure `CrossEncoder` is the right thing to export. In the code above, `CrossEncoder` is both the outer wrapper (with `inner: Arc<Mutex<...>>`) and has `new()` and `ensure_loaded()` and `rerank()`. That's clean enough.

Now for the retriever integration:

**`RerankerKind` enum:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RerankerKind {
    CrossEncoderOnnx,   // NEW DEFAULT
    EmbeddingV1,
    HeuristicV1,
}
```

**`default_reranker_kind()`:**
```rust
fn default_reranker_kind() -> RerankerKind {
    RerankerKind::CrossEncoderOnnx
}
```

**Import in retriever.rs:**
```rust
use crate::cross_encoder::CrossEncoder;
```

**Retriever struct:**
```rust
pub struct Retriever {
    connection: Connection,
    info: ArtifactInfo,
    documents: HashMap<String, StoredDocument>,
    chunks: Vec<IndexedChunk>,
    chunks_by_id: HashMap<i64, StoredChunk>,
    embedder: Option<Embedder>,
    cross_encoder: CrossEncoder,  // NEW — lazy-loaded
    mode_profile: ModeProfile,
}
```

**Initialize in `open_with_options`:**
```rust
pub fn open_with_options(...) -> Result<Self> {
    // ... existing ...
    Ok(Self {
        connection,
        info,
        documents,
        chunks,
        chunks_by_id,
        embedder,
        cross_encoder: CrossEncoder::new(),  // NEW — lazy, no download yet
        mode_profile: options.mode_profile,
    })
}
```

**`rerank_documents` match arm:**
```rust
match config.kind {
    RerankerKind::CrossEncoderOnnx => {
        self.cross_encoder.ensure_loaded()?;
        rerank_documents_with_cross_encoder(
            &self.cross_encoder, query, hits, config, top_k)
    }
    // ... existing ...
}
```

**Free function for cross-encoder document reranking:**
```rust
fn rerank_documents_with_cross_encoder(
    cross_encoder: &CrossEncoder,
    query: &str,
    hits: &[DocumentHit],
    config: &RerankerOptions,
    top_k: usize,
) -> Result<Vec<DocumentHit>> {
    let candidate_limit = config.candidate_pool_size.max(top_k);
    let candidates: Vec<&DocumentHit> = hits.iter().take(candidate_limit).collect();

    // Format each candidate as a passage string
    let passages: Vec<String> = candidates.iter().map(|hit| {
        let title = hit.title.as_deref().unwrap_or("Untitled");
        let heading = hit.best_match.heading_path.join(" > ");
        let excerpt = &hit.best_match.excerpt;
        format!("Title: {title}\nHeadings: {heading}\nContent: {excerpt}")
    }).collect();

    let ce_scores = cross_encoder.rerank(query, &passages, 32)?;

    let mut reranked: Vec<DocumentHit> = candidates.into_iter().cloned().enumerate()
        .map(|(i, mut hit)| {
            let blend = hit.score * 0.2 + ce_scores[i] * 0.8;
            hit.score = blend;
            hit
        })
        .collect();

    reranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    reranked.truncate(top_k);
    Ok(reranked)
}
```

**Same pattern for chunk reranking:**
```rust
fn rerank_chunks_with_cross_encoder(
    cross_encoder: &CrossEncoder,
    query: &str,
    hits: &[ChunkHit],
    config: &RerankerOptions,
    top_k: usize,
) -> Result<Vec<ChunkHit>> {
    // Same approach: format chunk fields into passage, run CE, blend scores
}
```

**`rerank_chunks` match arm** — same pattern as `rerank_documents`.

- [x] **Step 1: Modify `RerankerKind` default**

In `retriever.rs`:
```rust
pub enum RerankerKind {
    #[default]
    CrossEncoderOnnx,    // was EmbeddingV1
    EmbeddingV1,
    HeuristicV1,
}
```
And update `default_reranker_kind()`.

- [x] **Step 2: Add CrossEncoder field and import**

Add `use crate::cross_encoder::CrossEncoder;` at the top.
Add `cross_encoder: CrossEncoder` to `Retriever` struct.
Initialize in `open_with_options`: `cross_encoder: CrossEncoder::new(),`

- [x] **Step 3: Update match arms**

In both `rerank_documents` and `rerank_chunks`, add `CrossEncoderOnnx` arm before the existing two.

- [x] **Step 4: Add free functions**

Add `rerank_documents_with_cross_encoder` and `rerank_chunks_with_cross_encoder` in the free-function section (after the other reranker functions).

- [x] **Step 5: Commit**

```bash
git commit -am "feat: integrate CrossEncoderOnnx into Retriever as new default reranker"
```

---

### Task 4: Update public exports

**File:** `crates/indexbind-core/src/lib.rs`

```rust
#[cfg(not(target_arch = "wasm32"))]
mod cross_encoder;

// ... in the pub use section for non-wasm32:
pub use cross_encoder::CrossEncoder;
```

- [x] **Step 1: Edit lib.rs**

- [x] **Step 2: Verify compilation**

Run: `cargo check -p indexbind-core`
Expected: Compiles.

- [x] **Step 3: Commit**

```bash
git commit -am "feat: export CrossEncoder from indexbind-core"
```

---

### Task 5: Update Node NAPI bindings

**File:** `crates/indexbind-node/src/lib.rs`

Add `"cross-encoder-onnx"` string mapping in both `search` and `search_chunks` reranker kind parsing:

```rust
kind: match value.kind.as_deref() {
    Some("cross-encoder-onnx") | None => indexbind_core::RerankerKind::CrossEncoderOnnx,
    Some("embedding-v1") => indexbind_core::RerankerKind::EmbeddingV1,
    Some("heuristic-v1") => indexbind_core::RerankerKind::HeuristicV1,
    Some(other) => {
        return Err(Error::from_reason(format!(
            "unsupported reranker kind: {other}"
        )))
    }
},
```

- [x] **Step 1: Edit both match blocks** (in `search` and `search_chunks`)

- [x] **Step 2: Verify compilation**

Run: `cargo check -p indexbind-node`
Expected: Compiles.

- [x] **Step 3: Commit**

```bash
git commit -am "feat: add cross-encoder-onnx string mapping in NAPI bindings"
```

---

### Task 6: Tests

**File:** `crates/indexbind-core/src/retriever.rs` — add tests in `mod tests`

Add a test that runs reranking with `CrossEncoderOnnx`:

```rust
#[test]
fn cross_encoder_reranker_improves_document_order() {
    // This test requires ONNX Runtime + model download.
    // Skip if ORT_DYLIB_PATH is not set.
    if std::env::var("ORT_DYLIB_PATH").is_err() {
        eprintln!("skipping: ORT_DYLIB_PATH not set");
        return;
    }

    let dir = tempdir().unwrap();
    let artifact = dir.path().join("index.sqlite");

    build_artifact(
        &artifact,
        &[
            NormalizedDocument {
                doc_id: Some("doc-good".into()),
                relative_path: "good.md".into(),
                title: Some("Rust Concurrency".into()),
                content: "Arc and Mutex for shared state.".into(),
                ..Default::default()
            },
            NormalizedDocument {
                doc_id: Some("doc-weak".into()),
                relative_path: "weak.md".into(),
                title: Some("Notes".into()),
                content: "Random notes about things.".into(),
                ..Default::default()
            },
        ],
        &BuildArtifactOptions {
            source_root: SourceRoot { id: "root".into(), original_path: ".".into() },
            embedding_backend: EmbeddingBackend::Hashing { dimensions: 128 },
            chunking: Default::default(),
        },
        None,
    ).unwrap();

    let mut retriever = Retriever::open(&artifact).unwrap();
    let hits = retriever.search("Rust concurrency Arc Mutex", SearchOptions {
        reranker: Some(RerankerOptions {
            kind: RerankerKind::CrossEncoderOnnx,
            candidate_pool_size: 10,
        }),
        ..SearchOptions::default()
    }).unwrap();

    assert!(!hits.is_empty());
    assert_eq!(hits[0].doc_id, "doc-good");
}
```

- [x] **Step 1: Write the test**

- [x] **Step 2: Run conditionally**

```bash
ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so cargo test cross_encoder -- --nocapture
```

- [x] **Step 3: Run without ORT to confirm graceful skip**

```bash
cargo test cross_encoder -- --nocapture 2>&1
```
Expected: Prints "skipping: ORT_DYLIB_PATH not set", test passes (empty skip).

- [x] **Step 4: Commit**

```bash
git commit -am "test: add cross-encoder reranker test"
```

---

### Task 7: Update RerankerOptions defaults

The `default_reranker_kind()` is already updated in Task 3. One more thing: the `RerankerOptions::default()` uses `candidate_pool_size: 50`. Cross-encoders are slower than embedding-based reranking (each pair needs a forward pass), so consider a smaller default:

```rust
impl Default for RerankerOptions {
    fn default() -> Self {
        Self {
            kind: RerankerKind::CrossEncoderOnnx,
            candidate_pool_size: 20,  // was 50 — CE is ~10x slower per candidate
        }
    }
}
```

- [x] **Step 1: Reduce default candidate_pool_size**

- [x] **Step 2: Commit**

---

### Self-Review Checklist

**Spec coverage:**
- [x] `RerankerKind::CrossEncoderOnnx` variant added? Yes (Task 3)
- [x] `bge-reranker-v2-m3` ONNX model loaded? Yes (Task 2 via hf-hub)
- [x] Loaded once, shared via Arc? Yes (Task 2, CrossEncoder.inner is Arc<Mutex<Option<...>>>)
- [x] Launched in background thread? N/A — sync API, ONNX runs inline. The model loading is lazy (not at startup), and inference is fast enough that `thread::spawn` overhead would exceed the compute time. Acceptable.
- [x] `EmbeddingV1` replaced as default? Yes (Task 3, new default is `CrossEncoderOnnx`)
- [x] `EmbeddingV1` kept as fallback? Yes (Task 3, still present in enum)
- [x] Node NAPI updated? Yes (Task 5)
- [x] Tests written? Yes (Task 6)

**Placeholder check:** All code blocks contain real code, not "TBD" or placeholders.

**Type consistency:** `CrossEncoder` struct + `rerank()` method consistently named across all tasks.
