/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<92a8325986493ac23a53903908dc6169>>
 */


import type { BlobID } from './BlobID.ts';

/**
 * Maps each logical field of the graph to the [`BlobID`](s) that hold its
 * compressed data. A field may span multiple blobs if it exceeds the
 * configured chunk size.
 */
export interface ManifestBlobs {
  /** Concatenated UTF-8 node name bytes. */
  node_names: BlobID[];
  /** Offsets into the `node_names` byte array (one per node). */
  node_names_offsets: BlobID[];
  /** Flat list of directed-edge target indices (paired with `directed_offsets`). */
  directed: BlobID[];
  /** CSR-style offsets into `directed` (one per source node + 1 sentinel). */
  directed_offsets: BlobID[];
  /** Tagged edges (node → tag → set of target nodes). */
  tagged: BlobID[];
  /** Dynamic edges with runtime-defined branch labels. */
  dynamic: BlobID[];
  /** Per-metric float vectors (one `f64` per node for each named metric). */
  metrics: BlobID[];
  /** Per-label-name index (label-name → node → set of values). */
  labels: BlobID[];
  /** Per-property-name index (property-name → node → value). */
  properties: BlobID[];
  /** Optional traversal configuration (entry points, tier rules, etc.). */
  traversal_config: BlobID[];
  /** Explicit graph entry points, if set. */
  entry_points: BlobID[];
  /** Graph-level key-value properties (stored in manifest, not as blobs). */
  graph_properties: BlobID[];
}