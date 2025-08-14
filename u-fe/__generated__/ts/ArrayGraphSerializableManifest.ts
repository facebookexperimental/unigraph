/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { GraphSettings } from './GraphSettings.ts';
import type { ManifestBlobs } from './ManifestBlobs.ts';

export interface ArrayGraphSerializableManifest {
  blobs: ManifestBlobs;
  graph_settings?: GraphSettings | undefined;
}