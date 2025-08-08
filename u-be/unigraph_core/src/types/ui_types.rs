// Copyright (c) Meta Platforms, Inc. and affiliates.

#![allow(dead_code)]

#[derive(typegen::TypeGen)]
enum ExplorerComponentInputGraph {
    MapGraphSerialized(MapGraphSerialized),
    ArrayGraphSerialized(ArrayGraphSerialized),
}

#[derive(typegen::TypeGen)]
enum SerializationFormat {
    Json,
    JsonZstdBase64,
}

#[derive(typegen::TypeGen)]
struct MapGraphSerialized {
    format: SerializationFormat,
    value: String,
}

#[derive(typegen::TypeGen)]
struct ArrayGraphSerialized {
    format: SerializationFormat,
    value: String,
}
