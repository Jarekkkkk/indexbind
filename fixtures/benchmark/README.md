# Benchmark Fixtures

These fixtures provide a minimal retrieval regression baseline.

Current layout:

- `basic/docs/`: fixed source corpus used to build an artifact
- `basic/queries.json`: fixed queries and expected top document path

Example:

```bash
npx indexbind build fixtures/benchmark/basic/docs /tmp/indexbind-basic.sqlite hashing
npx indexbind benchmark /tmp/indexbind-basic.sqlite fixtures/benchmark/basic/queries.json
```
