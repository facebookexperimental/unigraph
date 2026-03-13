// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Timeline schema implementations.
//!
//! Each schema module encapsulates the fetch, compact, delete, and validation
//! logic for a specific [`TimelineSchema`](unigraph_storage_core::TimelineSchema)
//! variant. The [`namespaces::graph`](crate::namespaces::graph) module dispatches
//! to the appropriate schema at runtime.

pub(crate) mod adjacent_deltas;
pub(crate) mod full_or_delta;
