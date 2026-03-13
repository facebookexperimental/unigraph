/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Controls where graph blobs are stored for a timeline.
 * 
 * - `Inline`: blobs under the size threshold are compressed and stored
 *   directly in the frames table row. This is the default.
 * - `External`: blobs are always stored in the external blob storage,
 *   regardless of size.
 */
export type BlobStorageMode = "Inline" | "External";