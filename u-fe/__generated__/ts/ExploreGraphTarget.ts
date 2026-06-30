/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<e524d52d3e549a61f4fb2c36c7e612c4>>
 */


/** What to explore. */
export type ExploreGraphTarget =
  /** Auto-detected entry points (nodes with no parents). */
  { "EntryPoints": {  } } |
  /** Drill into a specific node's children. */
  { "Node": { name: string } } |
  /** Flat list of all reachable nodes. */
  { "AllNodes": {  } };

export type ExploreGraphTargetVariants = "EntryPoints" | "Node" | "AllNodes";