/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ArrayGraphSerializableManifest } from './ArrayGraphSerializableManifest.ts';
import type { BlobID } from './BlobID.ts';

/**
 * Base64-encoded variant of [`ArrayGraphSerializablePackage`].
 * 
 * JSON-serializing raw `Vec<u8>` produces a verbose integer array
 * (`[1, 19, 113, ...]`). This variant stores each blob as a base64
 * string instead, making it much more compact for JSON transport
 * between servers and browsers.
 */
export interface ArrayGraphSerializablePackageBase64 {
  manifest: ArrayGraphSerializableManifest;
  blobs_base_64: { [key: BlobID]: string };
}