/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ArrayGraphSerializableManifest } from './ArrayGraphSerializableManifest.ts';
import type { BlobID } from './BlobID.ts';

export interface ArrayGraphSerializablePackage {
  manifest: ArrayGraphSerializableManifest;
  blobs: { [key: BlobID]: number[] };
}