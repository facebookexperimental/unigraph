// Copyright (c) Meta Platforms, Inc. and affiliates.

// This module previously contained `pack_parallel`, a parallel version of
// `unigraph_core::pack`. That parallelism now lives directly in `pack` itself
// (via rayon::scope), so this module is no longer needed.
//
// All callers should use `ArrayGraphSerializable::pack()` or
// `unigraph_core::array_graph_serializable::package::pack()` directly.
