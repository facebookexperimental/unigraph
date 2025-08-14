// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  node_idx_to_name,
  node_name_to_idx_log,
} from "../.build/wasm/unigraph_wasm";
import type NativeGraph from "./NativeGraph";
import type { NodeIDXVecSet } from "./NativeGraph";
import type { NodeIDX } from "./types";

// This is a wrapper class over One or Two Native Graphs (left + ?right)

export default class TwinGraph {
  readonly l: NativeGraph;
  readonly r: NativeGraph | null;
  private entrypointsCache: NodeIDXVecSet | null = null;

  constructor(l: NativeGraph, r: NativeGraph | null) {
    this.l = l;
    this.r = r;
  }

  determineEntrypoints() {
    if (this.entrypointsCache == null) {
      if (this.r == null) {
        this.entrypointsCache = this.l.determineEntrypoints();
      } else {
        const resultR = this.r.determineEntrypoints();
        const resultL = this.l.determineEntrypoints();
        const combined = new Set<NodeIDX>([...resultL.set, ...resultR.set]);
        this.entrypointsCache = { vec: Array.from(combined), set: combined };
      }
    }
    return this.entrypointsCache;
  }

  getNodeName(nodeIDX: NodeIDX): string {
    // Node names are shared so it should not matter which graph we're
    // getting it from
    return node_idx_to_name(nodeIDX);
  }

  // This function is O(log(n)) because it uses a binary search.
  // We can use it for small to medium lookups, but it will get
  // pretty slow for very large lookups
  getNodeIDXByNameLog(name: string): NodeIDX | null {
    return node_name_to_idx_log(name) ?? null;
  }
}
