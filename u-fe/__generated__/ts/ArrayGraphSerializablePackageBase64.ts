/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ArrayGraphSerializableManifest } from './ArrayGraphSerializableManifest.ts';
import type { BlobID } from './BlobID.ts';

/**
 * Base64 Version of the package, where the blobs are Base64 encoded.
 * This is intended for browser use or JSON encoding of the package itself
 * for passing between servers/clients.
 * If Vec<u8> is double serialized into JSON it will be represented as
 *     [1, 19, 113, 48, ...]
 * which is very inefficient
 */
export interface ArrayGraphSerializablePackageBase64 {
  manifest: ArrayGraphSerializableManifest;
  blobs_base_64: { [key: BlobID]: string };
}