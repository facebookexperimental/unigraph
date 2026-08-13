// Copyright (c) Meta Platforms, Inc. and affiliates.

//! What history recorded about a frame it could not read.
//!
//! The ingest state itself lives in
//! [`unigraph_storage_core::IngestState`] — it is a stored column, so it
//! belongs with the row that carries it. This is the part that never reaches a
//! column: the payload is serialized into blob storage and the checkpoint keeps
//! only its key.

/// Serialized into the blob `HistoryStatusRow::error_blob_key` points at.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ErrorPayload {
    pub messages: Vec<String>,
    pub details: Option<String>,
}
