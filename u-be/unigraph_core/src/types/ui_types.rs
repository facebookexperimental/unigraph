// Copyright (c) Meta Platforms, Inc. and affiliates.

use unigraph_serialization::SerializedStr;

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

    /// Base GraphQueryConfig as JSON (from API response, immutable).
    /// Used as the baseline for delta computation.
    pub base_gqc_l: Option<String>,
    pub base_gqc_r: Option<String>,

    /// GQC delta (zstd+base64) — only the fields the user changed
    /// relative to the base GQC. Stored in the URL.
    pub gqc_delta_l: Option<String>,
    pub on_gqc_delta_change_l: Option<CallbackFn>,
    pub gqc_delta_r: Option<String>,
    pub on_gqc_delta_change_r: Option<CallbackFn>,

    /// Serialized graph settings (zstd+base64).
    pub graph_settings: Option<String>,
    pub on_graph_settings_change: CallbackFn,

    /// If set, the sidebar shows a home icon linking to this URL.
    /// Omit for standalone/local mode where there's no home page.
    pub home_href: Option<String>,
}

#[derive(typegen::TypeGen, serde::Deserialize)]
#[typegen(skip(Hack))]
pub struct ExplorerComponentInputGraphs {
    pub left: ExplorerComponentInputGraph,
    pub right: Option<ExplorerComponentInputGraph>,
}

#[derive(typegen::TypeGen, serde::Serialize, serde::Deserialize)]
#[typegen(skip(Hack))]
pub enum ExplorerComponentInputGraph {
    MapGraphSerialized(SerializedStr),
    ArrayGraphSerialized(SerializedStr),
    ArrayGraphSerializedPackageBase64(SerializedStr),
}
