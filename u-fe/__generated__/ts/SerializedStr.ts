/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<4cf6aa35c8387ec7cd9e9187fae681ab>>
 */


import type { SerializationFormat } from './SerializationFormat.ts';

/**
 * Struct that represents a value that has been serialized using provided
 * serialization format.
 * This is just a convenient wrapper around the serialized data that can be
 * passed around (and double serialized as part of a larger payload)
 */
export interface SerializedStr {
  data: string;
  format: SerializationFormat;
  /** Optional value of the initial type that was serialized. Used for debugging */
  type_hint?: string | undefined;
}