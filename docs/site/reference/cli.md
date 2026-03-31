---
title: CLI
order: 20
date: 2026-03-25
summary: Commands for building, inspecting, searching, and benchmarking indexbind artifacts.
---

# CLI

Install the npm package, then run the public CLI through `npx indexbind ...` or your package manager's local bin shim.

Main commands:

- `npx indexbind build [input-dir] [output-file] [--backend <hashing|model-id>]`
- `npx indexbind build-bundle [input-dir] [output-dir] [--backend <hashing|model-id>]`
- `npx indexbind update-cache [input-dir] [cache-file] [--backend <hashing|model-id>] [--git-diff] [--git-base <rev>]`
- `npx indexbind export-artifact <output-file> [--cache-file <path>]`
- `npx indexbind export-bundle <output-dir> [--cache-file <path>]`
- `npx indexbind inspect <artifact-file>`
- `npx indexbind search <artifact-file> <query> [flags]`
- `npx indexbind benchmark <artifact-file> <queries-json>`

Examples:

```bash
npx indexbind build ./docs
npx indexbind build . ./index.sqlite --backend hashing
npx indexbind build-bundle ./docs
npx indexbind update-cache ./docs --git-diff
npx indexbind export-artifact ./index.sqlite --cache-file ./docs/.indexbind/build-cache.sqlite
npx indexbind export-bundle ./index.bundle --cache-file ./docs/.indexbind/build-cache.sqlite
npx indexbind inspect ./docs/.indexbind/index.sqlite
npx indexbind search ./docs/.indexbind/index.sqlite "rust guide"
npx indexbind search ./docs/.indexbind/index.sqlite "rust guide" --text
npx indexbind benchmark ./docs/.indexbind/index.sqlite fixtures/benchmark/basic/queries.json
```

## Output Mode

Commands print JSON by default.

Add `--text` when you want a compact terminal-oriented summary instead:

```bash
npx indexbind inspect ./docs/.indexbind/index.sqlite --text
npx indexbind search ./docs/.indexbind/index.sqlite "rust guide" --text
```

This default is intentional so agents, shell scripts, and CI jobs can consume CLI output without extra parsing.

Default path rules:

- omitted `input-dir` means the current directory
- omitted `output-file`, `output-dir`, or `cache-file` for build commands writes under `<input-dir>/.indexbind/`
- `export-*` still requires an explicit output path; omit `--cache-file` to use `./.indexbind/build-cache.sqlite`

Scan defaults:

- hidden files and directories are ignored
- nested `.gitignore` rules are respected
- common generated or dependency directories such as `node_modules/`, `target/`, `dist/`, and `build/` are ignored

Embedding backend selection:

- pass `--backend hashing`
- or pass any `model2vec` model id with `--backend <model-id>`

If `--backend` is omitted, the current default backend is used.

## Incremental Cache Flow

Recommended sequence:

1. `update-cache` to refresh the mutable build cache
2. `export-artifact` to write a fresh SQLite artifact
3. `export-bundle` to write a fresh canonical bundle when needed

`update-cache` defaults to a full directory scan. Add `--git-diff` to use Git as a change-detection fast path. Add `--git-base <rev>` when you want to diff against a specific revision and still reuse the same cache.

## Search Flags

Use `search` to experiment with retrieval settings against a built SQLite artifact.

Supported flags:

- `--top-k <n>`
- `--mode <hybrid|vector|lexical>`
- `--reranker embedding-v1|heuristic-v1`
- `--candidate-pool-size <n>`
- `--relative-path-prefix <prefix>`
- `--metadata key=value` (repeatable)
- `--score-adjust-metadata-multiplier <field>`
- `--min-score <float>`
- `--text`

Example:

```bash
npx indexbind search ./docs/.indexbind/index.sqlite "rust guide" \
  --top-k 5 \
  --mode vector \
  --reranker heuristic-v1 \
  --candidate-pool-size 25 \
  --min-score 0.05 \
  --text
```

- `--mode vector` means vector-only retrieval.
- `--mode lexical` means lexical-only retrieval.

## Trigger Example

One simple local hook pattern is updating the cache after branch changes:

```bash
#!/usr/bin/env bash
set -euo pipefail

npx indexbind update-cache ./docs --git-diff
npx indexbind export-artifact ./index.sqlite --cache-file ./docs/.indexbind/build-cache.sqlite
```

This is only an adapter example. The cache logic still lives in the shared incremental engine, so the same flow can also be called from agent scripts, task runners, or a file watcher.
