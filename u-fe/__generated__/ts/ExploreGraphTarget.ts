/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<0c34e49bb9fc6106c86dbf08518a80c5>>
 */


import type { NodeSelection } from './NodeSelection.ts';

/** What to explore. */
export type ExploreGraphTarget =
  /** Auto-detected entry points (nodes with no parents). */
  { "EntryPoints": {  } } |
  /** Drill into a specific node's children. */
  { "Node": { name: string } } |
  /** Flat list of all reachable nodes. */
  { "AllNodes": {  } } |
  /**
   * Flat list of the reachable nodes matching `selection` — by name,
   * properties, or edge tags.
   */
  { "Matching": { selection: NodeSelection } };

export type ExploreGraphTargetVariants = "EntryPoints" | "Node" | "AllNodes" | "Matching";