// Copyright (c) Meta Platforms, Inc. and affiliates.

#[derive(typegen::TypeGen)]
#[typegen(TypeScript("(tvc: string) => void"), Flow("(string) => void"))]
pub struct CallbackFn();

#[derive(typegen::TypeGen)]
#[typegen(skip(Hack))]
pub struct ExplorerProps {
    /// NODE: DO NOT FORGET TO MEMOIZE IF YOU CONSTRUCT THIS OBJECT.
    ///
    /// Provide a graph to visualize/explore. Can be a single graph
    /// or two graphs that will be compared to each other.
    pub graphs: ExplorerComponentInputGraphs,

    /// serialized traversal config. Serialization format
    /// 1. JSON
    /// 2. ZSTD compression
    /// 3. Base64 (UrlSafe, NoPadding)
    pub traversal_config: Option<String>,
    pub on_traversal_config_change: CallbackFn,

    /// serialized traversal config. Serialization format
    /// 1. JSON
    /// 2. ZSTD compression
    /// 3. Base64 (UrlSafe, NoPadding)
    pub graph_settings: Option<String>,
    pub on_graph_settings_change: CallbackFn,
}

#[derive(typegen::TypeGen, serde::Deserialize)]
#[typegen(skip(Hack))]
pub struct ExplorerComponentInputGraphs {
    pub left: ExplorerComponentInputGraph,
    pub right: Option<ExplorerComponentInputGraph>,
}

#[derive(typegen::TypeGen, serde::Deserialize)]
#[typegen(skip(Hack))]
pub enum ExplorerComponentInputGraph {
    MapGraphSerialized(MapGraphSerialized),
    ArrayGraphSerialized(ArrayGraphSerialized),
}

#[derive(typegen::TypeGen, serde::Deserialize)]
#[typegen(skip(Hack))]
pub enum SerializationFormat {
    Json,
    JsonZstdBase64,
}

#[derive(typegen::TypeGen, serde::Deserialize)]
#[typegen(skip(Hack))]
pub struct MapGraphSerialized {
    pub format: SerializationFormat,
    pub value: String,
}

#[derive(typegen::TypeGen, serde::Deserialize)]
#[typegen(skip(Hack))]
pub struct ArrayGraphSerialized {
    pub format: SerializationFormat,
    pub value: String,
}
