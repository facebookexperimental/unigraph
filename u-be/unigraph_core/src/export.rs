// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Exporting a live graph to external file formats.
//!
//! Two orthogonal axes keep this modular:
//!   - [`ExportScope`]  — WHICH nodes/edges to include.
//!   - [`ExportFormat`] — HOW to serialize them.
//!
//! Both funnel through a [`MapGraph`] intermediate, so format writers never
//! think about filtering and scope logic never thinks about serialization.
//! Adding a new scope or format is a single match arm.
//!
//! ```text
//!   ArrayGraph ──scope──▶ MapGraph ──format──▶ Vec<u8>
//!               Reachable            MapGraphJson
//!               Whole                Gephi (GEXF)
//! ```

mod gexf;

use std::str::FromStr;

use anyhow::Result;
use anyhow::anyhow;

use crate::ArrayGraph;

/// Serialization target for an exported graph.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    PartialEq
)]
pub enum ExportFormat {
    /// Human-readable, round-trippable [`MapGraph`](crate::MapGraph) JSON — the
    /// same format the tool ingests, so exports reload directly.
    MapGraphJson,
    /// Gephi Graph Exchange XML Format (GEXF 1.3).
    Gephi,
}

/// Which part of the graph to export.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    PartialEq
)]
pub enum ExportScope {
    /// Only nodes reachable under the applied traversal config, with excluded
    /// edges (and edges to unreachable nodes) trimmed.
    Reachable,
    /// The entire graph as-is, ignoring traversal config.
    Whole,
}

/// Export `graph` (already reflecting any applied traversal config) to the given
/// `scope` and `format`, returning the file bytes ready to be written or
/// downloaded.
pub fn export_graph_bytes(
    graph: &ArrayGraph,
    scope: ExportScope,
    format: ExportFormat,
) -> Result<Vec<u8>> {
    let map_graph = match scope {
        ExportScope::Reachable => graph.to_configured_map_graph()?,
        ExportScope::Whole => graph.to_map_graph()?,
    };
    Ok(match format {
        ExportFormat::MapGraphJson => map_graph.to_json()?.into_bytes(),
        ExportFormat::Gephi => gexf::map_graph_to_gexf(&map_graph).into_bytes(),
    })
}

impl FromStr for ExportFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "MapGraphJson" => Ok(Self::MapGraphJson),
            "Gephi" => Ok(Self::Gephi),
            other => Err(anyhow!("unknown export format: `{other}`")),
        }
    }
}

impl FromStr for ExportScope {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "Reachable" => Ok(Self::Reachable),
            "Whole" => Ok(Self::Whole),
            other => Err(anyhow!("unknown export scope: `{other}`")),
        }
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::GraphBuilder;
    use crate::TraversalConfig;
    use crate::traversal::Decision;

    /// Build a small graph, exclude one edge, and export both scopes/formats.
    /// One table gives a bird's-eye view of every combination.
    #[test]
    fn test_export_graph_matrix() -> Result<()> {
        let task = ll::Task::create_new("test");
        let mut b = GraphBuilder::new();
        b.add_edge("A", "B").unwrap();
        b.add_edge("A", "C").unwrap();
        b.add_edge("B", "D").unwrap();
        // C -> E is the only way to reach E; excluding it makes E unreachable.
        b.add_edge("C", "E").unwrap();
        let mut graph = b.build().to_array_graph(&task)?;

        // Exclude the A -> C edge; C (and transitively E) drop out of the
        // reachable view.
        let mut force_edges = std::collections::BTreeMap::new();
        let mut a_edges = std::collections::BTreeMap::new();
        a_edges.insert("C".to_string(), Decision::exclude());
        force_edges.insert("A".to_string(), a_edges);
        let config = TraversalConfig {
            force_nodes: None,
            force_edges: Some(force_edges),
            force_tagged: None,
            label_predicates: None,
            force_dynamic: None,
            tiered_traversal: None,
            messages: None,
        };
        graph.apply_traversal_config_and_entry_points(config)?;

        let reachable_json = String::from_utf8(export_graph_bytes(
            &graph,
            ExportScope::Reachable,
            ExportFormat::MapGraphJson,
        )?)?;
        let reachable_gexf = String::from_utf8(export_graph_bytes(
            &graph,
            ExportScope::Reachable,
            ExportFormat::Gephi,
        )?)?;
        let whole_json = String::from_utf8(export_graph_bytes(
            &graph,
            ExportScope::Whole,
            ExportFormat::MapGraphJson,
        )?)?;

        // Reachable export drops C (excluded) and E (unreachable via C only).
        let reachable = crate::MapGraph::from_json(&reachable_json)?;
        let reachable_nodes: Vec<&str> = reachable.nodes.keys().map(String::as_str).collect();
        assert_eq!(reachable_nodes, vec!["A", "B", "D"]);
        // Whole export keeps every node.
        let whole = crate::MapGraph::from_json(&whole_json)?;
        let whole_nodes: Vec<&str> = whole.nodes.keys().map(String::as_str).collect();
        assert_eq!(whole_nodes, vec!["A", "B", "C", "D", "E"]);

        snapshot!(
            reachable_gexf,
            r#"
<?xml version="1.0" encoding="UTF-8"?>
<gexf xmlns="http://gexf.net/1.3" version="1.3">
<meta>
<creator>unigraph</creator>
</meta>
<graph mode="static" defaultedgetype="directed">
<nodes>
<node id="0" label="A">
</node>
<node id="1" label="B">
</node>
<node id="2" label="D">
</node>
</nodes>
<edges>
<edge id="0" source="0" target="1"/>
<edge id="1" source="1" target="2"/>
</edges>
</graph>
</gexf>

"#
        );
        Ok(())
    }
}
