/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

import type { BlobID } from "./BlobID.ts";

export interface ManifestStats {
  total_blobs: number;
  total_size_bytes: number;
  blob_sizes_bytes: { [key: BlobID]: number };
  node_count: number;
  directed_edge_count: number;
}
