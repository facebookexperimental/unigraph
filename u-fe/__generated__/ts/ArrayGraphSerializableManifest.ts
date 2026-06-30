/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<c5fa4229be9890aa26f897fdc61a6c1e>>
 */


import type { BlobID } from './BlobID.ts';
import type { GraphSettings } from './GraphSettings.ts';
import type { ManifestBlobs } from './ManifestBlobs.ts';
import type { ManifestStats } from './ManifestStats.ts';

/**
 * Metadata about the blob layout of a serialized graph.
 * 
 * When an [`ArrayGraphSerializable`] is packed via [`pack`], it is split into
 * individually compressed blobs. This manifest records which [`BlobID`]s
 * correspond to which graph fields, along with size statistics, so that the
 * graph can later be reassembled from blob storage without loading the
 * entire dataset at once.
 */
export interface ArrayGraphSerializableManifest {
  /**
   * [`BlobID`] that points to the JSON-serialized manifest itself, so the
   * manifest can be stored alongside the other blobs in the same store.
   */
  self_reference: BlobID;
  /** Aggregate statistics about the package (total size, blob count, etc.). */
  stats: ManifestStats;
  /** Per-field blob references that map each graph component to its blob(s). */
  blobs: ManifestBlobs;
  /** Optional graph-level settings (e.g. display configuration). */
  graph_settings?: GraphSettings | undefined;
  /** Graph-level key-value properties (not per-node). */
  properties: { [key: string]: string };
}