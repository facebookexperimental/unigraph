// Copyright (c) Meta Platforms, Inc. and affiliates.

import wasmBase64String from "../.build/wasm/unigraph_wasm_base64";

import { initSync } from "../.build/wasm/unigraph_wasm";

let INITIALIZED = false;

export default function initWasm() {
  if (INITIALIZED) {
    return;
  }
  // Decode the Base64 string to a binary string
  const binaryString = atob(wasmBase64String);
  // Convert the binary string to a Uint8Array
  const binaryArray = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    binaryArray[i] = binaryString.charCodeAt(i);
  }

  const wasmModule = new WebAssembly.Module(binaryArray);
  initSync(wasmModule);
  INITIALIZED = true;
}
