export function initSync(input: { module: WebAssembly.Module } | WebAssembly.Module): unknown;

export class WasmIndex {
  constructor(
    manifest: unknown,
    documents: unknown,
    chunks: unknown,
    vectors: Uint8Array,
    postings: unknown,
    tokenizerBytes?: Uint8Array,
    modelBytes?: Uint8Array,
    configBytes?: Uint8Array,
  );

  info(): unknown;
  search(query: string, options?: unknown): unknown;
}
