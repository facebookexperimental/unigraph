// Copyright (c) Meta Platforms, Inc. and affiliates.

//! End-to-end test utilities for Unigraph.
//!
//! Provides [`TestApp`](support::app::TestApp) for initializing a full
//! in-memory app instance. Tests go through the RPC layer (`exec_rpc`)
//! and snapshot nicely formatted results.

pub mod support;

#[cfg(test)]
mod tests;
