// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(clippy::collapsible_if)]

pub use crate::prepare_for_storage::StorageConfig;
pub use crate::prepare_for_storage::from_blobs;
pub use crate::prepare_for_storage::to_blobs;

mod manifest;
mod prepare_for_storage;
