---
title: Benchmarks and Case Studies
order: 25
date: 2026-03-29
summary: Review an indicative local baseline and the current in-house usage patterns behind indexbind.
---

# Benchmarks and Case Studies

This page is intentionally modest.

It is not trying to prove that `indexbind` wins every search benchmark. It is here to show:

- what a reproducible local baseline currently looks like
- how large the current docs corpus is
- which in-house usage patterns already rely on `indexbind`

## Indicative Local Baseline

These measurements were captured locally on a Darwin x86_64 development machine using the current `docs/site` corpus and the `hashing` embedding backend.

Treat them as indicative rather than universal. Hardware, Rust target cache state, document shape, embedding backend, and reranking choices all affect the numbers.

### Docs Site Baseline

Corpus shape:

- 14 markdown documents
- 74 chunks
- `hashing` backend
- native SQLite artifact

Observed local baseline:

| Metric | Value |
| --- | --- |
| Build command | `target/debug/indexbind-build build docs/site <tmp>/docs-site.sqlite hashing` |
| Build time | `0.07s` |
| Artifact size | `352 KB` |
| Query sample | 25 local Node searches with `hybrid: true` and `heuristic-v1` reranking |
| Average query latency | `3.61 ms` |
| Min / max query latency | `2.51 ms` / `5.28 ms` |

This is the current "small real corpus" baseline for the project's own documentation set.

### Regression Fixture Baseline

The bundled regression fixture is intentionally tiny and should be read as a correctness baseline, not a throughput benchmark.

Fixture shape:

- 3 documents
- 6 chunks
- fixed query set in `fixtures/benchmark/basic/queries.json`

Observed local baseline:

| Metric | Value |
| --- | --- |
| Build + benchmark command | `npm run benchmark:basic` |
| Benchmark result | `3 / 3` expected top hits passed |
| End-to-end benchmark run time | `0.01s` for the benchmark step after artifact build |

This fixture is useful for regression detection, CI confidence, and release checks. It is not meant to stand in for a large-corpus performance claim.

## Current In-House Case Studies

Current usage is still mostly first-party. That is fine at this stage, but it is worth stating plainly.

### Documentation Site

`indexbind` powers its own documentation story:

- docs are a fixed markdown corpus
- the corpus can be built into retrieval artifacts during publish
- the host site still owns navigation, rendering, and information architecture

This is the clearest public example of the docs-site/browser bundle path.

### Blog and Publishing Flow

`indexbind` is also used in a blog-style publishing flow where the host system owns:

- frontmatter and canonical URL decisions
- content routing
- product-level ranking policy

This is the clearest in-house example of the programmatic build path.

### Local Knowledge Base and Workspace Search

The project is also being used in a workspace-style local knowledge-base setting where:

- incremental updates matter
- agent or hook-triggered refreshes matter
- the host wants a local retrieval layer rather than a full mutable local-store product

This is the clearest in-house example of the incremental cache plus local Node artifact path.

## How to Read These Results

- The benchmark section is a local baseline, not a universal promise.
- The case-study section shows the product boundary `indexbind` is optimized for.
- The strongest current story is still "embedded retrieval for host-controlled systems", not "drop-in search for every environment".

If you want concrete wiring examples, continue to [Adoption Examples](./adoption-examples.md).
