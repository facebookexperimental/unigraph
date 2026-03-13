/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { BlobStorageMode } from './BlobStorageMode.ts';
import type { ExternalIDNamespace } from './ExternalIDNamespace.ts';
import type { TimelineSchema } from './TimelineSchema.ts';

/** Timeline configuration stored as a JSON blob in the `timelines` table. */
export interface TimelineConfig {
  schema: TimelineSchema;
  /**
   * Optional namespace for external ID mappings. When set, this timeline's
   * GraphIDs can be resolved back to ExternalIDs via the mapping table.
   */
  external_id_namespace?: ExternalIDNamespace | undefined;
  /**
   * Controls whether blobs are stored inline or always externally.
   * Defaults to `Inline` (blobs under 50 KB are stored in the frames table).
   */
  blob_storage: BlobStorageMode;
  /**
   * When `Some(true)`, per-node metric history is stored alongside each
   * graph frame in the same transaction. History blobs are partitioned by
   * ISO week for bounded blob sizes.
   */
  store_metric_history?: boolean | undefined;
}