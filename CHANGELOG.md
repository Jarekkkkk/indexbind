# Changelog

## 0.2.0

- Added canonical file-bundle build and runtime support for `indexbind/web`.
- Added programmatic bundle building via `indexbind/build`.
- Added wasm-backed query runtime coverage for Node workers, browsers, and Cloudflare Workers.
- Added `indexbind/cloudflare` for Cloudflare Worker environments that require static wasm module imports.
- Removed automatic JS fallback from `indexbind/web`; web runtimes now require wasm initialization to succeed.
- Added bundle smoke regressions for web, worker, browser, and Cloudflare Worker environments.
- `model2vec` web bundles now include model assets in the artifact bundle instead of relying on host filesystem access.
