/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


// A parsed graph handle — resolves to a cached or fetched `ArrayGraph`.
// 
// Handles come in three forms:
// - `gqc_{hash}` — GQC key (content-addressed config with embedded graph ref)
// - `{timeline}~{id}` — GraphKey (specific snapshot)
// - `{timeline}` — TimelineID (latest graph)
export type GraphHandle = string;
