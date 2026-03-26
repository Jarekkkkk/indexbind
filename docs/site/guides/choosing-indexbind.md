---
title: Choosing indexbind
order: 5
date: 2026-03-26
summary: Decide when indexbind fits better than a hosted search service, a static-site search tool, or a local knowledge-base product.
---

# Choosing indexbind

`indexbind` is best understood by what boundary it chooses.

It builds a retrieval artifact offline, then opens that artifact locally inside Node, browsers, or Workers.

That means it is optimized for embedding search into another product, not for operating a search service.

## Good Fit

Choose `indexbind` when you want to:

- build search from a fixed document collection
- keep the build step deterministic
- ship retrieval as part of your own CLI, site, app, or worker
- reuse the same retrieval behavior across Node and web runtimes
- avoid a runtime dependency on a hosted search API

Common examples:

- docs systems
- markdown publishing pipelines
- local tools and agent products
- browser or worker apps that need a portable search bundle

## Not the Best Fit

`indexbind` is usually not the first tool to reach for when you want:

- a hosted search service with operational features
- a full local knowledge-base product with its own ingestion workflow
- static-site-only search where the main goal is dropping in an existing UI package

## Quick Comparison

### `indexbind` vs `Pagefind`

Use `Pagefind` when your main goal is static-site search as a packaged product.

Use `indexbind` when search is one part of a larger system and you want to control how artifacts are built, loaded, filtered, and ranked across runtimes.

### `indexbind` vs `qmd`

Use `qmd` when you want an opinionated local knowledge-base search product.

Use `indexbind` when you are building your own product and want an embeddable retrieval layer rather than a full end-user workflow.

This difference also shows up in cost shape. `indexbind` can work well with a lighter embedding backend and let lexical retrieval, hybrid fusion, reranking, and product-specific ranking controls carry more of the relevance burden. On CPU-only machines, that can make local indexing far more practical than a stack that assumes heavier GGUF embedding models.

That is not a claim that `indexbind` always has the strongest semantic model. It is a claim that the retrieval stack gives you more engineering room to balance quality, startup cost, index build time, and runtime footprint.

### `indexbind` vs `Meilisearch`

Use `Meilisearch` when you want a service boundary, server-managed indexes, and service-style deployment.

Use `indexbind` when you want offline artifact builds and local runtime retrieval without depending on a remote search service.

## Runtime Model in One Sentence

`indexbind` moves indexing work into the build step so runtime code can stay small and embeddable.

## Next Step

- If this matches your use case, go to [Getting Started](./getting-started.md).
- If you already know you want `indexbind`, go directly to [API](../reference/api.md).
