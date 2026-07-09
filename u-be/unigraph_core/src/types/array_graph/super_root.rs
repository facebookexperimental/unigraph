// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::ArrayGraphSerializable;
use crate::types::NodeIDX;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
use crate::types::array_graph::remap_utils::make_remapped_node_names_ordered;
use crate::types::array_graph::remap_utils::remap_edges;
use crate::types::array_graph::remap_utils::remap_node_metadata;
use crate::types::array_graph::remap_utils::sort_and_return_mapping;

const SUPER_ROOT_BASE: &str = "root";
const SUPER_ROOT_WRAP: char = '~';
const MAX_APPEND_ATTEMPTS: usize = 3;

fn make_super_root_name(attempt: usize) -> String {
    let tildes: String = std::iter::repeat_n(SUPER_ROOT_WRAP, attempt + 1).collect();
    format!("{tildes}{SUPER_ROOT_BASE}{tildes}")
}

/// If the graph has more than one entrypoint it makes it very annoying to work with.
/// To make it easier we add a super root node that becomes a single entrypoint
/// with directed edges to all original entrypoints.
///
/// Tries to append a `~root~` node (which sorts after most real node names).
/// If that name doesn't sort last, cascades to `~~root~~`, `~~~root~~~`.
/// After 3 failed attempts, falls back to inserting the node at the correct
/// sorted position with a full index remap.
pub fn append_super_root(mut ag: ArrayGraph, force: bool) -> Result<ArrayGraph> {
    let entrypoints = ag.determine_entrypoints();

    if entrypoints.len() < 2 && !force {
        return Ok(ag);
    }

    for attempt in 0..MAX_APPEND_ATTEMPTS {
        let name = make_super_root_name(attempt);
        if Arc::make_mut(&mut ag.data)
            .node_names_ordered
            .append_node_name(&name)
            .is_ok()
        {
            return finish_append(ag, name, &entrypoints);
        }
    }

    let name = find_unique_name(&ag);
    insert_with_remap(ag, &name, &entrypoints)
}

fn finish_append(
    mut ag: ArrayGraph,
    super_root_name: String,
    entrypoints: &[NodeIDX],
) -> Result<ArrayGraph> {
    ag.runtime.node_flags.push(NodeFlags::empty());

    let data = Arc::make_mut(&mut ag.data);
    for &entrypoint in entrypoints {
        data.edges.edges.push(entrypoint);
        ag.runtime.edge_flags.push(EdgeFlags::empty());
    }
    data.edges.edge_offsets.push(data.edges.edges.len());

    data.node_metadata
        .metrics
        .values_mut()
        .for_each(|m| m.push(0.0));

    data.entry_points = Some(BTreeSet::from([super_root_name]));

    ag.runtime.derived_state = ArrayGraphDerivedState::new();

    Ok(ag)
}

fn find_unique_name(ag: &ArrayGraph) -> String {
    (0..)
        .map(make_super_root_name)
        .find(|name| ag.data.node_names_ordered.name_to_idx_log(name).is_none())
        .expect("infinite tilde names available, one must be unused")
}

fn insert_with_remap(
    ag: ArrayGraph,
    super_root_name: &str,
    entrypoints: &[NodeIDX],
) -> Result<ArrayGraph> {
    // Sole owner in the append path → moves the inner value out; falls back to
    // a one-time deep clone only if the data is shared.
    let data = Arc::unwrap_or_clone(ag.data);

    let mut names: Vec<String> = data
        .node_names_ordered
        .node_names_iter()
        .map(|s| s.to_string())
        .collect();
    names.push(super_root_name.to_string());

    let mut edges = data.edges;
    for &ep in entrypoints {
        edges.edges.push(ep);
    }
    edges.edge_offsets.push(edges.edges.len());

    let mut metadata = data.node_metadata;
    for metrics in metadata.metrics.values_mut() {
        metrics.push(0.0);
    }

    let ctx = sort_and_return_mapping(&mut names);

    let new_sg = ArrayGraphSerializable {
        node_names_ordered: make_remapped_node_names_ordered(&names),
        edges: remap_edges(&edges, &ctx)?,
        node_metadata: remap_node_metadata(&metadata, &ctx)?,
        graph_settings: data.graph_settings,
        traversal_config: data.traversal_config,
        entry_points: Some(BTreeSet::from([super_root_name.to_string()])),
        properties: data.properties,
    };

    new_sg
        .into_array_graph(&ll::Task::create_new(""))
        .context("Failed to reconstruct ArrayGraph after super root remap")
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::snapshot;

    use super::*;
    use crate::GraphBuilder;

    #[test]
    fn test_make_super_root_name() {
        snapshot!(make_super_root_name(0), "~root~");
        snapshot!(make_super_root_name(1), "~~root~~");
        snapshot!(make_super_root_name(2), "~~~root~~~");
    }

    fn make_graph(edges: &[(&str, &str)]) -> Result<ArrayGraph> {
        let mut b = GraphBuilder::new();
        for &(from, to) in edges {
            b.add_edge(from, to)?;
        }
        b.build().to_array_graph(&ll::Task::create_new("test"))
    }

    #[test]
    fn test_normal_append() -> Result<()> {
        let ag = make_graph(&[("A", "B"), ("C", "D")])?;
        let ag = ag.append_super_root(false)?;

        snapshot!(
            ag.debug().to_forward_edges_string()?,
            "
A:
  - B
B:
C:
  - D
D:
~root~:
  - A
  - C
"
        );
        Ok(())
    }

    #[test]
    fn test_single_entrypoint_no_force() -> Result<()> {
        let ag = make_graph(&[("A", "B"), ("B", "C")])?;
        let ag = ag.append_super_root(false)?;

        snapshot!(
            ag.debug().to_forward_edges_string()?,
            "
A:
  - B
B:
  - C
C:
"
        );
        Ok(())
    }

    #[test]
    fn test_single_entrypoint_force() -> Result<()> {
        let ag = make_graph(&[("A", "B"), ("B", "C")])?;
        let ag = ag.append_super_root(true)?;

        snapshot!(
            ag.debug().to_forward_edges_string()?,
            "
A:
  - B
B:
  - C
C:
~root~:
  - A
"
        );
        Ok(())
    }

    #[test]
    fn test_cascading_name() -> Result<()> {
        let mut b = GraphBuilder::new();
        b.add_edge("A", "B")?;
        b.add_edge("C", "D")?;
        b.add_node("~root~".to_string());
        let ag = b.build().to_array_graph(&ll::Task::create_new("test"))?;
        let ag = ag.append_super_root(false)?;

        snapshot!(
            ag.debug().to_forward_edges_string()?,
            "
A:
  - B
B:
C:
  - D
D:
~root~:
~~root~~:
  - A
  - C
  - ~root~
"
        );
        Ok(())
    }

    #[test]
    fn test_remap_fallback() -> Result<()> {
        let mut b = GraphBuilder::new();
        b.add_edge("A", "B")?;
        b.add_edge("C", "D")?;
        b.add_node("~~~~z".to_string());
        let ag = b.build().to_array_graph(&ll::Task::create_new("test"))?;
        let ag = ag.append_super_root(false)?;

        snapshot!(
            ag.debug().to_forward_edges_string()?,
            "
A:
  - B
B:
C:
  - D
D:
~root~:
  - A
  - C
  - ~~~~z
~~~~z:
"
        );
        Ok(())
    }
}
