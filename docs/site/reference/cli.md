---
title: CLI
order: 20
date: 2026-03-25
summary: Commands for building, inspecting, and benchmarking indexbind artifacts.
---

# CLI

Current CLI lives in the Rust `indexbind-build` crate.

Main commands:

- `cargo run -p indexbind-build -- build <input-dir> <output-file> [hashing|<model-id>]`
- `cargo run -p indexbind-build -- build-bundle <input-dir> <output-dir> [hashing|<model-id>]`
- `cargo run -p indexbind-build -- update-cache <input-dir> <cache-file> [hashing|<model-id>] [--git-diff] [--git-base <rev>]`
- `cargo run -p indexbind-build -- export-artifact <cache-file> <output-file>`
- `cargo run -p indexbind-build -- export-bundle <cache-file> <output-dir>`
- `cargo run -p indexbind-build -- inspect <artifact-file>`
- `cargo run -p indexbind-build -- benchmark <artifact-file> <queries-json>`

Examples:

```bash
cargo run -p indexbind-build -- build ./docs ./index.sqlite
cargo run -p indexbind-build -- build-bundle ./docs ./index.bundle
cargo run -p indexbind-build -- update-cache ./docs ./.indexbind-cache.sqlite --git-diff
cargo run -p indexbind-build -- export-artifact ./.indexbind-cache.sqlite ./index.sqlite
cargo run -p indexbind-build -- export-bundle ./.indexbind-cache.sqlite ./index.bundle
cargo run -p indexbind-build -- inspect ./index.sqlite
cargo run -p indexbind-build -- benchmark ./index.sqlite fixtures/benchmark/basic/queries.json
```

Embedding backend selection:

- `hashing`
- any other string is treated as a `model2vec` model id

If the backend argument is omitted, the current default backend is used.

## Incremental Cache Flow

Recommended sequence:

1. `update-cache` to refresh the mutable build cache
2. `export-artifact` to write a fresh SQLite artifact
3. `export-bundle` to write a fresh canonical bundle when needed

`update-cache` defaults to a full directory scan. Add `--git-diff` to use Git as a change-detection fast path. Add `--git-base <rev>` when you want to diff against a specific revision and still reuse the same cache.

## Trigger Example

One simple local hook pattern is updating the cache after branch changes:

```bash
#!/usr/bin/env bash
set -euo pipefail

cargo run -p indexbind-build -- update-cache ./docs ./.indexbind-cache.sqlite --git-diff
cargo run -p indexbind-build -- export-artifact ./.indexbind-cache.sqlite ./index.sqlite
```

This is only an adapter example. The cache logic still lives in the shared incremental engine, so the same flow can also be called from agent scripts, task runners, or a file watcher.
