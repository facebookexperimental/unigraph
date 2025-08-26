/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

import type { SerializedStr } from "./SerializedStr.ts";

export type ExplorerComponentInputGraph =
  | { MapGraphSerialized: SerializedStr }
  | { ArrayGraphSerialized: SerializedStr }
  | { ArrayGraphSerializedPackage: SerializedStr };

export type ExplorerComponentInputGraphVariants =
  | "MapGraphSerialized"
  | "ArrayGraphSerialized"
  | "ArrayGraphSerializedPackage";
