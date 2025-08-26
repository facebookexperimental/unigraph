/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { BlobID } from './BlobID.ts';
import type { GraphSettings } from './GraphSettings.ts';
import type { ManifestBlobs } from './ManifestBlobs.ts';
import type { ManifestStats } from './ManifestStats.ts';

/**
 * ArrayGraphSerializable can be serialized and chunked into multiple compressed blobs
 * This manifest provides all the necessary metadata to locate and deserialize these blobs
 * back into the initial graph.
 */
export interface ArrayGraphSerializableManifest {
  /** Blob ID for the manifest itself serialized as JSON */
  self_reference: BlobID;
  stats: ManifestStats;
  blobs: ManifestBlobs;
  graph_settings?: GraphSettings | undefined;
}