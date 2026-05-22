// Copyright (c) Meta Platforms, Inc. and affiliates.

use unigraph_serialization::SerializedStr;

#[derive(serde::Deserialize)]
pub struct ExplorerComponentInputGraphs {
    pub left: Option<ExplorerComponentInputGraph>,
    pub right: ExplorerComponentInputGraph,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum ExplorerComponentInputGraph {
    MapGraphSerialized(SerializedStr),
    ArrayGraphSerialized(SerializedStr),
    ArrayGraphSerializedPackageBase64(SerializedStr),
}
