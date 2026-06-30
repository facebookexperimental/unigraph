/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<3123c435e8dababdc97577cff170e233>>
 */


import type { BlobID } from './BlobID.ts';

/** Summary statistics recorded at pack time. */
export interface ManifestStats {
  /** Total number of data blobs (excludes the manifest blob itself). */
  total_blobs: number;
  /** Sum of all blob sizes in bytes (compressed). */
  total_size_bytes: number;
  /** Per-blob compressed size map. */
  blob_sizes_bytes: { [key: BlobID]: number };
  /** Number of nodes in the graph. */
  node_count: number;
  /** Number of directed edges in the graph. */
  directed_edge_count: number;
}