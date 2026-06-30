/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<0c88972ebd3f601c63246a1c33ce33da>>
 */


// A parsed graph handle — three ways to reference a graph.
// 
// Handles come in three forms:
// - `gqc_{hash}` — GQC key (content-addressed config with embedded graph ref)
// - `{timeline}~{id}` — GraphKey (specific snapshot)
// - `{timeline}` — TimelineID (latest graph)
export type GraphHandle = string;
