/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Unique identifier for a blob within a graph package.
 * 
 * Each blob holds a compressed chunk of serialized graph data (e.g. node names,
 * edges, metrics). The ID is typically derived from the field name and a hash
 * of the blob contents (e.g. `"directed_1506826171969472540"`).
 */
export type BlobID = string;