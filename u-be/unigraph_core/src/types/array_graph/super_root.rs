// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::array_graph_nodes::ArrayGraphNodesForGraphSide;
use crate::types::array_graph::offset_graph::Edge;
use crate::types::array_graph::offset_graph::OffsetGraph;

const HIGHEST_UNICODE_CODEPOINT: u32 = 0x10FFFF;

/// If the graph has more than one entrypoint it makes it very very annoying to work with
/// in most cases. Like... if i load it in the UI i would want to know the transitive size
/// of "the whole thing" but when there are multiple entrypoints i would only be able to
/// see the separate values for each entrypoints and not combined.
/// To make it easier we can add a super root node that will become a single entrypoint
/// for the whole graph. It will have directed edges to the multiple entrypoints we had initially.
///
/// Since most of the time our graphs are immutable / append-only we'll have to do some sketchy stuff
/// to actually add the super root node.
/// The name of the super root will be prefixed with the highest unicode code point
/// so that it is always the last node in the ordered list of node names. It is not guaranteed
/// but should probably work in most cases, unless someone actually used these characters in the actual
/// graph node names.
pub fn append_super_root(
    ag: ArrayGraph,
    // force super root creation even if there's only one entrypoint
    force: bool,
) -> Result<ArrayGraph> {
    let entrypoints = ag.determine_entrypoints();

    if entrypoints.len() < 2 && !force {
        return Ok(ag); // No need to add super root if there's only one entrypoint
    }

    let ArrayGraph {
        nodes,
        mut node_flags,
        mut edges_forward,
        derived_state: _, // this is invalidated and need to be recomputed
        edges_tagged,
        edges_dynamic,
        mut metrics,
        tag_sets,
        graph_settings,
        budget_configs,
        entry_points: _, // we get them by determining
        state,
    } = ag;

    let highest_unicode_char =
        char::from_u32(HIGHEST_UNICODE_CODEPOINT).context("Failed to get highest unicode char")?;
    let super_root_name = format!("{highest_unicode_char}__root__{highest_unicode_char}");

    let mut node_names_ordered = match Arc::try_unwrap(nodes.node_names) {
        Ok(node_names) => node_names,
        Err(arc) => (*arc).clone(),
    };

    node_names_ordered
        .append_node_name(&super_root_name)
        .context(
        "Failed to add super root node name. Super root name uses the highest unicode code point
prefix to become append-only last node in the ordered list but it is not guaranteed that there will
be no other node already on the list that doesn't start from the same character",
    )?;

    node_flags.push(NodeFlags::empty());
    append_super_root_edges(entrypoints, &mut edges_forward);
    metrics.values_mut().for_each(|m| m.push(0.0));

    let derived_state = ArrayGraphDerivedState::from_forward_edges(&edges_forward);

    let nodes = ArrayGraphNodesForGraphSide::new_left_only(Arc::new(node_names_ordered));

    Ok(ArrayGraph {
        nodes,
        node_flags,
        edges_forward,
        derived_state,
        edges_tagged,
        edges_dynamic,
        metrics,
        tag_sets,
        state,
        graph_settings,
        budget_configs,
        entry_points: Some(BTreeSet::from([super_root_name])),
    })
}

/// Since the super root is the last node we can just append its new edges
/// to the end of the edges_forward.
fn append_super_root_edges(entrypoints: Vec<NodeIDX>, edges_forward: &mut OffsetGraph) {
    for entrypoint in entrypoints {
        edges_forward.edges.push(Edge {
            points_to: entrypoint,
            flags: Default::default(),
        });
        edges_forward
            .non_directed_edges_metadata
            .push(crate::types::array_graph::offset_graph::NonDirectedEdgeMetadata::Directed);
    }
    edges_forward.edge_offsets.push(edges_forward.edges.len());
}
