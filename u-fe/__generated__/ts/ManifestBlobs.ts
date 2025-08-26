/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { BlobID } from './BlobID.ts';

/** Contains references to all individual blobs */
export interface ManifestBlobs {
  node_names: BlobID[];
  node_names_offsets: BlobID[];
  directed: BlobID[];
  directed_offsets: BlobID[];
  tagged: BlobID[];
  dynamic: BlobID[];
  metrics: BlobID[];
  tag_sets: BlobID[];
  traversal_config: BlobID[];
  entry_points: BlobID[];
}