---
title: API
order: 10
date: 2026-03-25
summary: Node, build, web, and Cloudflare entrypoints, plus the current search option surface.
---

# API

`indexbind` has four runtime-facing entrypoints:

- `indexbind`
- `indexbind/build`
- `indexbind/web`
- `indexbind/cloudflare`

## `indexbind`

Native Node entrypoint for SQLite artifacts:

```ts
import { openIndex } from 'indexbind';

const index = await openIndex('./index.sqlite');
const hits = await index.search('rust guide', {
  topK: 5,
  hybrid: true,
  reranker: { kind: 'embedding-v1', candidatePoolSize: 25 },
  relativePathPrefix: 'guides/',
});
```

### `openIndex(artifactPath)`

Opens a native SQLite artifact and returns an `Index`.

### `index.info()`

Returns artifact metadata such as:

- `schemaVersion`
- `builtAt`
- `embeddingBackend`
- `lexicalTokenizer`
- `sourceRoot`
- `documentCount`
- `chunkCount`

### `index.search(query, options?)`

Main options:

- `topK?`: number of hits to return
- `hybrid?`: combine lexical and vector retrieval
- `reranker?`: optional final reranking stage
- `relativePathPrefix?`: restrict retrieval to a path subtree
- `metadata?`: exact-match metadata filter
- `scoreAdjustment?`: adjust final ranking using metadata-driven multipliers

On the Node entrypoint, `metadata` is currently exposed as a string-to-string map in the TypeScript API.

Reranker options:

- `kind?`: `embedding-v1` or `heuristic-v1`
- `candidatePoolSize?`: candidate count forwarded into the reranker before final `topK`

Score-adjustment options:

- `metadataNumericMultiplier?`: metadata field name whose numeric value should multiply the final score

The returned hits include:

- `docId`
- `relativePath`
- `canonicalUrl?`
- `title?`
- `summary?`
- `metadata`
- `score`
- `bestMatch`

`bestMatch` contains:

- `chunkId`
- `excerpt`
- `headingPath`
- `charStart`
- `charEnd`
- `score`

## `indexbind/build`

Programmatic canonical bundle build API:

```ts
import { buildCanonicalBundle } from 'indexbind/build';
```

Main input shape:

- `docId?`
- `sourcePath?`
- `relativePath`
- `canonicalUrl?`
- `title?`
- `summary?`
- `content`
- `metadata?`

Use this entrypoint when your host application already has a normalized document set and wants to build a canonical bundle directly.

## `indexbind/web`

Browser and worker entrypoint for canonical bundles:

```ts
import { openWebIndex } from 'indexbind/web';
```

This path requires wasm initialization to succeed.

`openWebIndex(base)` returns a `WebIndex`.

`WebIndex.info()` returns canonical bundle metadata such as:

- `schemaVersion`
- `artifactFormat`
- `builtAt`
- `embeddingBackend`
- `documentCount`
- `chunkCount`
- `vectorDimensions`
- `chunking`
- `features`

`WebIndex.search(query, options?)` accepts the same search options as the Node entrypoint, except metadata values can use the broader JSON value shape.

## `indexbind/cloudflare`

Cloudflare Worker entrypoint:

```ts
import { openWebIndex } from 'indexbind/cloudflare';
```

Use this instead of `indexbind/web` inside Workers so wasm can be loaded through the Worker-compatible static module path.

## Search Defaults and Patterns

Reasonable starting point:

```ts
const hits = await index.search(query, {
  topK: 10,
  hybrid: true,
  reranker: {
    kind: 'embedding-v1',
    candidatePoolSize: 25,
  },
});
```

Use metadata filtering when your host application has clear product boundaries:

```ts
const hits = await index.search(query, {
  metadata: {
    lang: 'rust',
    visibility: 'public',
  },
});
```

Use metadata-based score adjustment when your application wants a host-defined ranking prior:

```ts
const hits = await index.search(query, {
  scoreAdjustment: {
    metadataNumericMultiplier: 'directory_weight',
  },
});
```

For a fuller explanation of how these knobs interact, see [Search Quality Controls](../guides/search-quality-controls.md).
