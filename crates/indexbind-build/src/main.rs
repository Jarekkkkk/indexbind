use anyhow::{anyhow, bail, Result};
use indexbind_build::{
    build_canonical_from_directory, build_from_directory, export_artifact_from_cache,
    export_canonical_from_cache, update_cache_from_directory_with_mode, DirectoryUpdateMode,
};
use indexbind_core::{
    BuildArtifactOptions, EmbeddingBackend, IncrementalBuildStats, Retriever, SearchOptions,
};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(command_or_input) = args.next() else {
        bail!("{}", usage());
    };

    match command_or_input.as_str() {
        "build" => build_command(args.collect()),
        "build-bundle" => build_bundle_command(args.collect()),
        "update-cache" => update_cache_command(args.collect()),
        "export-artifact" => export_artifact_command(args.collect()),
        "export-bundle" => export_bundle_command(args.collect()),
        "inspect" => inspect_command(args.collect()),
        "benchmark" => benchmark_command(args.collect()),
        input => build_command_with_input(input.to_string(), args.collect()),
    }
}

fn build_bundle_command(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let input = args.next().ok_or_else(|| anyhow!("{}", usage()))?;
    let output = args.next().ok_or_else(|| anyhow!("{}", usage()))?;
    let backend = match args.next().as_deref() {
        Some("hashing") => EmbeddingBackend::Hashing { dimensions: 256 },
        Some(model) => EmbeddingBackend::Model2Vec {
            model: model.to_string(),
            batch_size: 512,
        },
        None => EmbeddingBackend::default(),
    };

    let stats = build_canonical_from_directory(
        &PathBuf::from(input),
        &PathBuf::from(output),
        BuildArtifactOptions {
            embedding_backend: backend,
            ..Default::default()
        },
    )?;

    println!(
        "built canonical artifact bundle with {} documents, {} chunks, and {}-dim vectors",
        stats.document_count, stats.chunk_count, stats.vector_dimensions
    );
    Ok(())
}

fn build_command(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let input = args.next().ok_or_else(|| anyhow!("{}", usage()))?;
    build_command_with_input(input, args.collect())
}

fn update_cache_command(args: Vec<String>) -> Result<()> {
    let mut positional = Vec::new();
    let mut backend_arg = None;
    let mut use_git_diff = false;
    let mut git_base = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--git-diff" => use_git_diff = true,
            "--git-base" => {
                git_base = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("--git-base requires a revision"))?,
                );
                use_git_diff = true;
            }
            value if value == "hashing" || (!value.starts_with("--") && positional.len() >= 2) => {
                backend_arg = Some(arg);
            }
            _ => positional.push(arg),
        }
    }
    let input = positional
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("{}", usage()))?;
    let cache_path = positional
        .get(1)
        .cloned()
        .ok_or_else(|| anyhow!("{}", usage()))?;
    let backend = parse_embedding_backend(backend_arg);
    let mode = if use_git_diff {
        DirectoryUpdateMode::GitDiff {
            base_revision: git_base,
        }
    } else {
        DirectoryUpdateMode::FullScan
    };
    let stats = update_cache_from_directory_with_mode(
        &PathBuf::from(input.clone()),
        &PathBuf::from(cache_path.clone()),
        BuildArtifactOptions {
            embedding_backend: backend,
            ..Default::default()
        },
        mode,
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&incremental_stats_json(&stats))?
    );
    Ok(())
}

fn export_artifact_command(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let cache_path = args.next().ok_or_else(|| anyhow!("{}", usage()))?;
    let output = args.next().ok_or_else(|| anyhow!("{}", usage()))?;
    let stats = export_artifact_from_cache(&PathBuf::from(cache_path), &PathBuf::from(output))?;
    println!(
        "exported artifact with {} documents and {} chunks",
        stats.document_count, stats.chunk_count
    );
    Ok(())
}

fn export_bundle_command(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let cache_path = args.next().ok_or_else(|| anyhow!("{}", usage()))?;
    let output = args.next().ok_or_else(|| anyhow!("{}", usage()))?;
    let stats = export_canonical_from_cache(&PathBuf::from(cache_path), &PathBuf::from(output))?;
    println!(
        "exported canonical artifact bundle with {} documents, {} chunks, and {}-dim vectors",
        stats.document_count, stats.chunk_count, stats.vector_dimensions
    );
    Ok(())
}

fn build_command_with_input(input: String, args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let output = args.next().ok_or_else(|| anyhow!("{}", usage()))?;
    let backend = parse_embedding_backend(args.next());

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

fn inspect_command(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let artifact = args
        .next()
        .ok_or_else(|| anyhow!("usage: indexbind-build inspect <artifact-file>"))?;
    let retriever = Retriever::open(&PathBuf::from(artifact))?;
    let info = retriever.info();
    let payload = json!({
        "schemaVersion": info.schema_version,
        "builtAt": info.built_at,
        "embeddingBackend": info.embedding_backend,
        "lexicalTokenizer": info.lexical_tokenizer,
        "sourceRoot": info.source_root,
        "documentCount": info.document_count,
        "chunkCount": info.chunk_count,
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn benchmark_command(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let artifact = args.next().ok_or_else(|| {
        anyhow!("usage: indexbind-build benchmark <artifact-file> <queries-json>")
    })?;
    let queries_path = args.next().ok_or_else(|| {
        anyhow!("usage: indexbind-build benchmark <artifact-file> <queries-json>")
    })?;
    let payload = fs::read_to_string(&queries_path)?;
    let fixture: BenchmarkFixture = serde_json::from_str(&payload)?;
    let mut retriever = Retriever::open(&PathBuf::from(artifact))?;

    let mut passed = 0usize;
    let mut results = Vec::new();
    for case in &fixture.queries {
        let hits = retriever.search(
            &case.query,
            SearchOptions {
                top_k: case.top_k.unwrap_or(5),
                ..SearchOptions::default()
            },
        )?;
        let top_hit = hits.first().map(|hit| hit.relative_path.clone());
        let success = top_hit.as_deref() == Some(case.expected_top_hit.as_str());
        if success {
            passed += 1;
        }
        results.push(json!({
            "name": case.name,
            "query": case.query,
            "expectedTopHit": case.expected_top_hit,
            "actualTopHit": top_hit,
            "passed": success,
        }));
    }

    let summary = json!({
        "fixture": fixture.name,
        "total": fixture.queries.len(),
        "passed": passed,
        "failed": fixture.queries.len().saturating_sub(passed),
        "results": results,
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn parse_embedding_backend(value: Option<String>) -> EmbeddingBackend {
    match value.as_deref() {
        Some("hashing") => EmbeddingBackend::Hashing { dimensions: 256 },
        Some(model) => EmbeddingBackend::Model2Vec {
            model: model.to_string(),
            batch_size: 512,
        },
        None => EmbeddingBackend::default(),
    }
}

fn incremental_stats_json(stats: &IncrementalBuildStats) -> serde_json::Value {
    json!({
        "scannedDocumentCount": stats.scanned_document_count,
        "newDocumentCount": stats.new_document_count,
        "changedDocumentCount": stats.changed_document_count,
        "unchangedDocumentCount": stats.unchanged_document_count,
        "removedDocumentCount": stats.removed_document_count,
        "activeDocumentCount": stats.active_document_count,
        "activeChunkCount": stats.active_chunk_count,
    })
}

#[derive(Debug, Deserialize)]
struct BenchmarkFixture {
    name: String,
    queries: Vec<BenchmarkQuery>,
}

#[derive(Debug, Deserialize)]
struct BenchmarkQuery {
    name: String,
    query: String,
    expected_top_hit: String,
    top_k: Option<usize>,
}

fn usage() -> &'static str {
    "usage:\n  indexbind-build build <input-dir> <output-file> [hashing|<model-id>]\n  indexbind-build build-bundle <input-dir> <output-dir> [hashing|<model-id>]\n  indexbind-build update-cache <input-dir> <cache-file> [hashing|<model-id>] [--git-diff] [--git-base <rev>]\n  indexbind-build export-artifact <cache-file> <output-file>\n  indexbind-build export-bundle <cache-file> <output-dir>\n  indexbind-build inspect <artifact-file>\n  indexbind-build benchmark <artifact-file> <queries-json>\n\nFor backward compatibility, `indexbind-build <input-dir> <output-file> [hashing|<model-id>]` still works."
}
