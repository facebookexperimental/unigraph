// Copyright (c) Meta Platforms, Inc. and affiliates.

/**
 * Public API surface for the Unigraph library bundle.
 *
 * This file is the entry point for the Rolldown build (`rolldown.config.ts`).
 * It produces a single self-contained ES module (`.build/js/Unigraph.js`) that
 * external consumers can import to embed the graph explorer in their own apps.
 *
 * The bundle includes everything needed to render: React components,
 * wasm-bindgen glue, and Tailwind CSS (injected at runtime). The WASM binary
 * (`unigraph_wasm_bg.wasm`) ships as a separate file alongside the bundle.
 * React itself is externalized — consumers must provide it.
 *
 * Usage:
 *
 *   import { initWasm, Explorer, RpcProvider, createFetchTransport } from "Unigraph";
 *
 *   await initWasm(); // call once at app startup
 *
 *   <RpcProvider transport={createFetchTransport("/my/api/rpc")}>
 *     <Explorer graphs={{ left: myGraph }} />
 *   </RpcProvider>
 */

// ---------------------------------------------------------------------------
// WASM initialization — must be called (and awaited) once before rendering.
// ---------------------------------------------------------------------------

export { default as initWasm } from "./init_wasm";

// ---------------------------------------------------------------------------
// Explorer component
// ---------------------------------------------------------------------------

export { Explorer } from "./Explorer";
export type { PanelTabPlugin, BuiltinSidebarPanel } from "./Explorer";

// ---------------------------------------------------------------------------
// RPC — typed client, React context, and Suspense hook
// ---------------------------------------------------------------------------

export {
  UnigraphRpc,
  RpcProvider,
  useRpc,
  useRpcCall,
  createFetchTransport,
} from "./api/rpc";
export type { RpcTransport, RpcMethod, RpcMethodMap } from "./api/rpc";

// ---------------------------------------------------------------------------
// Graph data — hooks for reading graph data inside panel tab plugins.
//
//   useTwinGraph()    — primary entry point. Gives the TwinGraph instance
//                       with node lookups, fuzzy search, arrows, shortest
//                       paths, and delta metrics. Access sides via .l / .r.
//   useNativeGraphL() / useNativeGraphR()
//                     — direct access to individual NativeGraph sides for
//                       metrics, tier info, reachability, etc.
//   useIsDeltaGraph() — quick check for comparison mode.
// ---------------------------------------------------------------------------

export {
  useTwinGraph,
  useNativeGraphL,
  useNativeGraphR,
  useNativeGraphs,
  useIsDeltaGraph,
} from "./context/NativeGraphContext";

// ---------------------------------------------------------------------------
// Traversal config — read/write the traversal configuration (entrypoints,
// force-included/excluded edges and nodes).
//
//   useTVC()  — returns { tvcL, setTvcL, tvcR, setTvcR }
// ---------------------------------------------------------------------------

export { useTVC } from "./context/TraversalConfigContext";
export type { TraversalConfigContextType } from "./context/TraversalConfigContext";

// ---------------------------------------------------------------------------
// Graph settings — display settings (metric selection, sort order, etc.)
//
//   useGraphSettings()  — returns [settings, setSettings]
// ---------------------------------------------------------------------------

export { useGraphSettings } from "./context/GraphSettingsContext";
export type { GraphSettingsContextType } from "./context/GraphSettingsContext";
