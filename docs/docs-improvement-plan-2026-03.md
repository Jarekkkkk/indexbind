# indexbind Docs Improvement Plan

Date: 2026-03-26

## Context

Current `indexbind` documentation explains the architecture direction, but it is still harder than it should be for a first-time user to answer four practical questions:

1. Why should I choose `indexbind` instead of `Pagefind`, `qmd`, or `Meilisearch`?
2. What is the fastest path from install to a successful query?
3. Which runtime entrypoint should I use for Node, browsers, or Workers?
4. Which knobs affect search quality today?

Compared with other search products, the gap is not a lack of concepts. The gap is product-facing guidance.

## Goals

- sharpen the positioning on the repo README and docs homepage
- provide a real five-minute quickstart instead of a command summary
- document the current quality-control surface of the search API
- make the docs site easier to navigate by user task, not only by section type

## Proposed Changes

### 1. Positioning

Add a short comparison page that explains where `indexbind` fits:

- compared with `Pagefind`: embedded library and retrieval artifact, not only static-site search
- compared with `qmd`: retrieval engine for embedding into other products, not a full local knowledge-base product
- compared with `Meilisearch`: offline artifact and embedded runtime, not a hosted search service

Also add a concise "best fit" and "not the best fit" section to the root README.

### 2. Getting Started

Rewrite `docs/site/guides/getting-started.md` around one minimal end-to-end path:

- install
- create a tiny document set
- build a native artifact
- query it from Node
- build a canonical bundle
- query it from the web runtime
- explain when to choose each artifact

This page should be executable, not only descriptive.

### 3. Search Quality Controls

Add a dedicated page for:

- `hybrid`
- `reranker.kind`
- `reranker.candidatePoolSize`
- `metadata` exact-match filtering
- `scoreAdjustment.metadataNumericMultiplier`

The goal is to explain which knobs affect recall, reranking, and final ordering.

### 4. API Reference

Expand `docs/site/reference/api.md` so it documents:

- entrypoints
- `search()` options
- return shape
- common patterns for Node and web runtimes

### 5. Docs Homepage

Update `docs/site/README.md` so the primary entrypoints are task-oriented:

- choose `indexbind`
- get a first query working
- tune retrieval quality

## Out of Scope

These should not be mixed into this pass:

- a full benchmark/comparison section with performance claims
- deep tokenizer internals
- detailed embedding model selection guidance
- a full API reference generated from types

## Success Criteria

- a new user can understand whether `indexbind` fits their use case from the README and docs homepage
- a new user can run one end-to-end local example without reading source code
- a product integrator can discover the current search-quality controls without opening `src/` or `crates/`
