// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::types::array_graph::NodeFlags;
use crate::types::array_graph::array_graph_derived_state::ArrayGraphDerivedState;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;

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
    mut ag: ArrayGraph,
    // force super root creation even if there's only one entrypoint
    force: bool,
) -> Result<ArrayGraph> {
    let entrypoints = ag.determine_entrypoints();

    if entrypoints.len() < 2 && !force {
        return Ok(ag); // No need to add super root if there's only one entrypoint
    }

    let highest_unicode_char =
        char::from_u32(HIGHEST_UNICODE_CODEPOINT).context("Failed to get highest unicode char")?;
    let super_root_name = format!("{highest_unicode_char}__root__{highest_unicode_char}");

    ag.data.node_names_ordered.append_node_name(&super_root_name).context(
        "Failed to add super root node name. Super root name uses the highest unicode code point
prefix to become append-only last node in the ordered list but it is not guaranteed that there will
be no other node already on the list that doesn't start from the same character",
    )?;

    ag.runtime.node_flags.push(NodeFlags::empty());

    // Append super root edges to CSR data and runtime edge_flags
    for &entrypoint in &entrypoints {
        ag.data.edges.edges.push(entrypoint);
        ag.runtime.edge_flags.push(EdgeFlags::empty());
    }
    ag.data.edges.edge_offsets.push(ag.data.edges.edges.len());

    ag.data
        .node_metadata
        .metrics
        .values_mut()
        .for_each(|m| m.push(0.0));

    // Invalidate derived state (reverse, dominator, SCCs)
    ag.runtime.derived_state = ArrayGraphDerivedState::new();

    ag.data.entry_points = Some(BTreeSet::from([super_root_name]));

    Ok(ag)
}
