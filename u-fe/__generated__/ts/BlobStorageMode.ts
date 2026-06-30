/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<e9df760449deb84243dd4fe7a8b929cc>>
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