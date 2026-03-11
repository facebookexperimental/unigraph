/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ArrayGraphSerializableManifest } from './ArrayGraphSerializableManifest.ts';
import type { BlobID } from './BlobID.ts';

/**
 * A fully self-contained graph package: the manifest plus all blob data.
 * 
 * Blobs are stored as raw `Vec<u8>` (ZSTD-compressed). This representation
 * is suitable for in-memory use or binary serialization.
 */
export interface ArrayGraphSerializablePackage {
  manifest: ArrayGraphSerializableManifest;
  blobs: { [key: BlobID]: number[] };
}