# Benchmark Fixtures

These fixtures provide a minimal retrieval regression baseline.

Current layout:

- `basic/docs/`: fixed source corpus used to build an artifact
- `basic/queries.json`: fixed queries and expected top document path

Example:

```bash
npm run build:cli
node dist/cli.js build fixtures/benchmark/basic/docs /tmp/indexbind-basic.sqlite --backend hashing
node dist/cli.js benchmark /tmp/indexbind-basic.sqlite fixtures/benchmark/basic/queries.json
```
