// Copyright (c) Meta Platforms, Inc. and affiliates.

/// see NodeDiff in unigraph_core/src/types/twin_graph/diff.rs
export type NodeDiff = number;

/**
 * Value that represents the things that changed about a node
 * between the left and right graphs of a TwinGraph.
 *
 * TypeScript implementation equivalent to the Rust bitflags::bitflags! NodeDiff
 */
export const NodeDiffFlags = {
  DOES_NOT_EXIST_IN_L: 0b0001,
  DOES_NOT_EXIST_IN_R: 0b0010,
  EDGES_CHANGED: 0b0100,
  METRICS_CHANGED: 0b1000,
} as const;

export function nodeDoesNotExistInL(diff: NodeDiff): boolean {
  return (diff & NodeDiffFlags.DOES_NOT_EXIST_IN_L) !== 0;
}

export function nodeDoesNotExistInR(diff: NodeDiff): boolean {
  return (diff & NodeDiffFlags.DOES_NOT_EXIST_IN_R) !== 0;
}

export function nodeEdgesChanged(diff: NodeDiff): boolean {
  return (diff & NodeDiffFlags.EDGES_CHANGED) !== 0;
}

export function nodeMetricsChanged(diff: NodeDiff): boolean {
  return (diff & NodeDiffFlags.METRICS_CHANGED) !== 0;
}
