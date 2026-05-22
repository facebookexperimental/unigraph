// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Roundtrip conversion test:
//!   MapGraph → ArrayGraphSerializable → ArrayGraph → ArrayGraphSerializable → MapGraph
//!
//! Exercises all node metadata (properties, labels, metrics), all edge types
//! (directed, tagged, dynamic), and top-level fields (graph_settings,
//! traversal_config, entry_points).

use anyhow::Result;
use k9::assert_equal;
use maplit::btreemap;
use maplit::btreeset;

use crate::MapGraph;
use crate::graph_settings::GraphSettings;
use crate::traversal::Decision;
use crate::traversal::TraversalConfig;
use crate::types::map_graph::DynamicEdge;
use crate::types::map_graph::GraphNode;

/// Builds a MapGraph fixture that exercises every feature:
///
/// ```text
///   A ──dir──► B ──tag "T1"──► C
///   │                          │
///   └──dir──► D ──tag "T2"──► E
///                              │
///   F ──dynamic(dk/de)──► G    │
///   │   branch b1              │
///   └──dynamic(dk/de)──► H    │
///       branch b2              │
///                              │
///   I (leaf, no edges)         │
///   E ──dir──► I
/// ```
///
/// Node metadata:
/// - A: properties={oncall: "team_a"}, labels={platform: {ios, android}}, metrics={size: 10.0}
/// - B: labels={platform: {ios}}, metrics={size: 5.0, count: 2.0}
/// - C: metrics={size: 3.0}
/// - D: properties={path: "/d"}, metrics={count: 1.0}
/// - E: labels={platform: {android}}, metrics={size: 7.0}
/// - F: properties={oncall: "team_f", path: "/f"}, labels={lang: {rust}}
/// - G: (no metadata)
/// - H: metrics={size: 1.0}
/// - I: labels={platform: {web}}, properties={owner: "nobody"}
fn make_conversion_test_graph() -> MapGraph {
    MapGraph {
        nodes: btreemap! {
            "A".into() => GraphNode {
                properties: Some(btreemap! { "oncall".into() => "team_a".into() }),
                labels: Some(btreemap! { "platform".into() => btreeset!{ "ios".into(), "android".into() } }),
                metrics: Some(btreemap! { "size".into() => 10.0 }),
                edges_directed: Some(btreeset! { "B".into(), "D".into() }),
                edges_tagged: None,
                edges_dynamic: None,
            },
            "B".into() => GraphNode {
                properties: None,
                labels: Some(btreemap! { "platform".into() => btreeset!{ "ios".into() } }),
                metrics: Some(btreemap! { "size".into() => 5.0, "count".into() => 2.0 }),
                edges_directed: None,
                edges_tagged: Some(btreemap! { "T1".into() => btreeset!{ "C".into() } }),
                edges_dynamic: None,
            },
            "C".into() => GraphNode {
                properties: None,
                labels: None,
                metrics: Some(btreemap! { "size".into() => 3.0 }),
                edges_directed: None,
                edges_tagged: None,
                edges_dynamic: None,
            },
            "D".into() => GraphNode {
                properties: Some(btreemap! { "path".into() => "/d".into() }),
                labels: None,
                metrics: Some(btreemap! { "count".into() => 1.0 }),
                edges_directed: None,
                edges_tagged: Some(btreemap! { "T2".into() => btreeset!{ "E".into() } }),
                edges_dynamic: None,
            },
            "E".into() => GraphNode {
                properties: None,
                labels: Some(btreemap! { "platform".into() => btreeset!{ "android".into() } }),
                metrics: Some(btreemap! { "size".into() => 7.0 }),
                edges_directed: Some(btreeset! { "I".into() }),
                edges_tagged: None,
                edges_dynamic: None,
            },
            "F".into() => GraphNode {
                properties: Some(btreemap! { "oncall".into() => "team_f".into(), "path".into() => "/f".into() }),
                labels: Some(btreemap! { "lang".into() => btreeset!{ "rust".into() } }),
                metrics: None,
                edges_directed: None,
                edges_tagged: None,
                edges_dynamic: Some(btreemap! {
                    "dk".into() => btreemap! {
                        "de".into() => DynamicEdge {
                            branches: btreemap! {
                                "b1".into() => btreeset!{ "G".into() },
                                "b2".into() => btreeset!{ "H".into() },
                            },
                            metadata: Some(btreemap! { "info".into() => "test".into() }),
                        },
                    },
                }),
            },
            "G".into() => GraphNode {
                properties: None,
                labels: None,
                metrics: None,
                edges_directed: None,
                edges_tagged: None,
                edges_dynamic: None,
            },
            "H".into() => GraphNode {
                properties: None,
                labels: None,
                metrics: Some(btreemap! { "size".into() => 1.0 }),
                edges_directed: None,
                edges_tagged: None,
                edges_dynamic: None,
            },
            "I".into() => GraphNode {
                properties: Some(btreemap! { "owner".into() => "nobody".into() }),
                labels: Some(btreemap! { "platform".into() => btreeset!{ "web".into() } }),
                metrics: None,
                edges_directed: None,
                edges_tagged: None,
                edges_dynamic: None,
            },
        },
        traversal_config: Some(TraversalConfig {
            force_nodes: Some(btreemap! {
                "G".into() => Decision { include: false, message_id: None },
            }),
            ..Default::default()
        }),
        graph_settings: Some(GraphSettings::default()),
        entry_points: Some(btreeset! { "A".into(), "F".into() }),
        properties: btreemap! {
            "source".into() => "test".into(),
            "version".into() => "1.0".into(),
        },
    }
}

#[test]
fn test_full_roundtrip() -> Result<()> {
    let original = make_conversion_test_graph();
    let original_json = serde_json::to_string_pretty(&original)?;

    // MapGraph → ArrayGraphSerializable → ArrayGraph → (to_map_graph) → MapGraph
    let serializable = original.to_array_graph_serializable()?;
    let array_graph = serializable.into_array_graph(&ll::Task::create_new(""))?;
    let roundtrip = array_graph.to_map_graph()?;
    let roundtrip_json = serde_json::to_string_pretty(&roundtrip)?;

    assert_equal!(
        k9::MultilineString(original_json.clone()),
        k9::MultilineString(roundtrip_json.clone())
    );

    Ok(())
}

#[test]
fn test_roundtrip_through_json_serializable() -> Result<()> {
    let original = make_conversion_test_graph();
    let original_json = serde_json::to_string_pretty(&original)?;

    // MapGraph → ArrayGraphSerializable → JSON → ArrayGraphSerializable → ArrayGraph → MapGraph
    let serializable = original.to_array_graph_serializable()?;
    let json = serializable.to_json()?;
    let deserialized = crate::ArrayGraphSerializable::from_json(&json)?;
    let array_graph = deserialized.into_array_graph(&ll::Task::create_new(""))?;
    let roundtrip = array_graph.to_map_graph()?;
    let roundtrip_json = serde_json::to_string_pretty(&roundtrip)?;

    assert_equal!(
        k9::MultilineString(original_json),
        k9::MultilineString(roundtrip_json)
    );

    Ok(())
}

#[test]
fn test_roundtrip_through_into_serializable() -> Result<()> {
    let original = make_conversion_test_graph();
    let original_json = serde_json::to_string_pretty(&original)?;

    // MapGraph → ArrayGraphSerializable → ArrayGraph → into_serializable() → ArrayGraph → MapGraph
    let serializable = original.to_array_graph_serializable()?;
    let ag = serializable.into_array_graph(&ll::Task::create_new(""))?;
    let serializable2 = ag.into_serializable();
    let ag2 = serializable2.into_array_graph(&ll::Task::create_new(""))?;
    let roundtrip = ag2.to_map_graph()?;
    let roundtrip_json = serde_json::to_string_pretty(&roundtrip)?;

    assert_equal!(
        k9::MultilineString(original_json),
        k9::MultilineString(roundtrip_json)
    );

    Ok(())
}
