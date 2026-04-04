// Copyright (c) Meta Platforms, Inc. and affiliates.

import __wbg_init from "../.build/wasm/unigraph_wasm";

let promise: Promise<void> | null = null;

/**
 * Initialize the WASM module. Call once before rendering any Unigraph components.
 *
 * Accepts an optional URL to the .wasm file. If omitted, defaults to
 * `unigraph_wasm_bg.wasm` relative to the JS bundle (via `import.meta.url`).
 */
export default function initWasm(wasmUrl?: string | URL): Promise<void> {
  if (!promise) {
    promise = __wbg_init(
      wasmUrl ? { module_or_path: wasmUrl } : undefined,
    ).then(() => {});
  }
  return promise;
}
