// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(dead_code)]

#[derive(typegen::TypeGen)]
#[typegen(TypeScript("(tvc: string) => void"), Flow("(string) => void"))]
struct CallbackFn();

#[derive(typegen::TypeGen)]
#[typegen(skip(Hack))]
struct ExplorerParams {
    pub graph_left: ExplorerComponentInputGraph,
    pub graph_right: Option<ExplorerComponentInputGraph>,
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

#[derive(typegen::TypeGen)]
#[typegen(skip(Hack))]
enum ExplorerComponentInputGraph {
    MapGraphSerialized(MapGraphSerialized),
    ArrayGraphSerialized(ArrayGraphSerialized),
}

#[derive(typegen::TypeGen)]
#[typegen(skip(Hack))]
enum SerializationFormat {
    Json,
    JsonZstdBase64,
}

#[derive(typegen::TypeGen)]
#[typegen(skip(Hack))]
struct MapGraphSerialized {
    format: SerializationFormat,
    value: String,
}

#[derive(typegen::TypeGen)]
#[typegen(skip(Hack))]
struct ArrayGraphSerialized {
    format: SerializationFormat,
    value: String,
}
