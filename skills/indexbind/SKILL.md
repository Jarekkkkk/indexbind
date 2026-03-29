---
name: indexbind
description: Use when an agent needs to install or use indexbind through its CLI or programming APIs in Node, browsers, Web Workers, or Cloudflare Workers. This skill helps choose the right package, artifact, and entrypoint, and links to the live markdown docs for details.
---

# Indexbind

Use this skill when the task is about **using** `indexbind` from a host application or environment.

## Install

For the JavaScript APIs:

```bash
npm install indexbind
```

For the current CLI:

- the CLI lives in the Rust `indexbind-build` crate
- it is not installed by `npm install indexbind`
- use it only when a Rust toolchain is available and the environment explicitly has the CLI available
- if a task does not require the CLI specifically, prefer the programmatic APIs from `indexbind/build`

CLI docs:
- `https://indexbind.jolestar.workers.dev/reference/cli.md`

General install and platform notes:
- `https://indexbind.jolestar.workers.dev/guides/getting-started.md`
- `https://indexbind.jolestar.workers.dev/reference/packaging.md`

## Choose the right interface

- Need local Node querying over a built artifact:
  use `indexbind`
- Need to build or update indexes from code:
  use `indexbind/build`
- Need browser or worker querying over a canonical bundle:
  use `indexbind/web`
- Need Cloudflare Worker querying:
  use `indexbind/cloudflare`
- Need shell-driven index construction or inspection:
  use the `indexbind-build` CLI if that Rust binary is available in the environment

## Artifact choice

- Node runtime:
  use a native SQLite artifact
- Browser, Web Worker, Cloudflare Worker:
  use a canonical bundle
- Repeated rebuilds over a stable corpus:
  use the build cache, then export fresh artifacts or bundles

Concepts:
- `https://indexbind.jolestar.workers.dev/concepts/runtime-model.md`
- `https://indexbind.jolestar.workers.dev/concepts/canonical-bundles.md`

## CLI shape

When the Rust CLI binary is available, the command surface is:

- `indexbind-build build ...`
- `indexbind-build build-bundle ...`
- `indexbind-build update-cache ...`
- `indexbind-build export-artifact ...`
- `indexbind-build export-bundle ...`
- `indexbind-build inspect ...`
- `indexbind-build benchmark ...`

Use the CLI when the host workflow is shell-driven or file-system driven. Otherwise prefer `indexbind/build`.

Docs:
- `https://indexbind.jolestar.workers.dev/reference/cli.md`

## Programming interfaces

Use these APIs when the host already has documents or wants tighter control:

- `openIndex(...)` from `indexbind`
- `buildCanonicalBundle(...)` from `indexbind/build`
- `updateBuildCache(...)` from `indexbind/build`
- `exportArtifactFromBuildCache(...)` from `indexbind/build`
- `exportCanonicalBundleFromBuildCache(...)` from `indexbind/build`
- `openWebIndex(...)` from `indexbind/web`
- `openWebIndex(...)` from `indexbind/cloudflare`

Docs:
- `https://indexbind.jolestar.workers.dev/reference/api.md`

## Cloudflare rule

Inside Cloudflare Workers:

- prefer `indexbind/cloudflare`
- if bundle files are not directly exposed as public URLs, pass a custom `fetch` to `openWebIndex(...)`
- use the host asset loader such as `ASSETS.fetch(...)` rather than monkey-patching global fetch

Docs:
- `https://indexbind.jolestar.workers.dev/guides/web-and-cloudflare.md`
- `https://indexbind.jolestar.workers.dev/reference/api.md`

## Search defaults

Reasonable starting point:

- `hybrid: true`
- `topK: 10`
- reranker:
  `embedding-v1` with `candidatePoolSize: 25`

Only add metadata filtering or score adjustment when the host app has a clear product rule for them.

Docs:
- `https://indexbind.jolestar.workers.dev/guides/search-quality-controls.md`
- `https://indexbind.jolestar.workers.dev/reference/api.md`

## Read in this order when unsure

1. `https://indexbind.jolestar.workers.dev/guides/getting-started.md`
2. `https://indexbind.jolestar.workers.dev/reference/api.md`
3. `https://indexbind.jolestar.workers.dev/reference/cli.md`
4. `https://indexbind.jolestar.workers.dev/guides/web-and-cloudflare.md`
