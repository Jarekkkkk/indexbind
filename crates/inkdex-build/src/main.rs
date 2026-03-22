use anyhow::{anyhow, Result};
use inkdex_build::build_from_directory;
use inkdex_core::{BuildArtifactOptions, EmbeddingBackend};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let input = args
        .next()
        .ok_or_else(|| anyhow!("usage: inkdex-build <input-dir> <output-file> [hashing|fastembed]"))?;
    let output = args
        .next()
        .ok_or_else(|| anyhow!("usage: inkdex-build <input-dir> <output-file> [hashing|fastembed]"))?;
    let backend = match args.next().as_deref() {
        Some("fastembed") => EmbeddingBackend::Fastembed {
            model_name: "intfloat/multilingual-e5-small".to_string(),
            model_code: "MultilingualE5Small".to_string(),
        },
        _ => EmbeddingBackend::Hashing { dimensions: 256 },
    };

    let stats = build_from_directory(
        &PathBuf::from(input),
        &PathBuf::from(output),
        BuildArtifactOptions {
            embedding_backend: backend,
            ..Default::default()
        },
    )?;

    println!(
        "built artifact with {} documents and {} chunks",
        stats.document_count, stats.chunk_count
    );
    Ok(())
}
