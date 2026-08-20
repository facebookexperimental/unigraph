// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Turning an [`ExploreGraphTarget`] into the merged-namespace rows a delta
//! table shows.

use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use unigraph_core::GraphSide;
use unigraph_core::NodeDiff;
use unigraph_core::NodeIDX;
use unigraph_core::NodeSelection;
use unigraph_core::TwinArrow;
use unigraph_core::TwinGraph;
use unigraph_core::graph_settings::GraphStructure;

use super::ExploreDeltaEdge;
use crate::rpc_req::ExploreGraphTarget;
use crate::rpc_req::explore_graph::ENUMERATE_ALL;
use crate::rpc_req::explore_graph::reject_top_k_name_mode;

/// One row, before metrics are computed.
pub struct DeltaRow {
    pub node_idx: NodeIDX,
    pub node_diff: NodeDiff,
    /// The edge leading here on each side. Both `None` for entry-point /
    /// all-node rows, which aren't reached via an edge.
    pub l: Option<ExploreDeltaEdge>,
    pub r: Option<ExploreDeltaEdge>,
    pub skipped: usize,
}

pub struct ResolvedRows {
    /// The node being drilled into, if any.
    pub parent_idx: Option<NodeIDX>,
    pub rows: Vec<DeltaRow>,
    /// Unchanged nodes filtered out of `rows`. Only non-zero for `AllNodes` and
    /// `Matching` in changed-nodes-only mode — for `Node` targets the collapsing
    /// shows up as per-row `skipped` counts instead.
    pub hidden_unchanged_count: usize,
}

pub fn resolve(
    tg: &TwinGraph,
    target: &ExploreGraphTarget,
    structure: GraphStructure,
    changed_nodes_only: bool,
    task: &ll::Task,
) -> Result<ResolvedRows> {
    match target {
        ExploreGraphTarget::EntryPoints {} => Ok(no_parent(entry_point_rows(tg))),
        ExploreGraphTarget::AllNodes {} => Ok(all_node_rows(tg, changed_nodes_only)),
        ExploreGraphTarget::Matching { selection } => {
            matching_rows(tg, selection, changed_nodes_only, task)
        }
        ExploreGraphTarget::Node { name } => child_rows(tg, name, structure, changed_nodes_only)
            .map(|(idx, rows)| ResolvedRows {
                parent_idx: Some(idx),
                rows,
                hidden_unchanged_count: 0,
            }),
    }
}

// ── Targets ─────────────────────────────────────────────────────

/// The union of both sides' entry points, mirroring `TwinGraph.determineEntrypoints`
/// in `u-fe/native/TwinGraph.tsx`. After super-rooting this is normally a single
/// `~root~`, but a side can contribute its own root when the two disagree.
fn entry_point_rows(tg: &TwinGraph) -> Vec<DeltaRow> {
    [GraphSide::Left, GraphSide::Right]
        .into_iter()
        .flat_map(|side| {
            tg.graph(side)
                .determine_entrypoints()
                .into_iter()
                .map(move |local_idx| tg.to_merged(side, local_idx))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|merged_idx| node_row(tg, merged_idx))
        .collect()
}

fn all_node_rows(tg: &TwinGraph, changed_nodes_only: bool) -> ResolvedRows {
    let reachable: Vec<NodeIDX> = tg
        .merged_node_idx_iter()
        .filter(|&idx| is_reachable_on_either_side(tg, idx))
        .collect();
    let total = reachable.len();

    let rows: Vec<DeltaRow> = reachable
        .into_iter()
        .filter(|&idx| !changed_nodes_only || tg.is_node_changed(idx))
        .map(|idx| node_row(tg, idx))
        .collect();

    ResolvedRows {
        parent_idx: None,
        hidden_unchanged_count: total - rows.len(),
        rows,
    }
}

/// Nodes matching `selection` on either side, in merged-namespace order.
///
/// Each side is matched against its own `ArrayGraph` — property indices and the
/// name list are per-side — and the two results are unioned. A node added or
/// removed between the graphs therefore still shows up, matched on whichever
/// side it exists.
///
/// `Fuzzy` is rejected for the same reason as in `explore_graph` — it cannot
/// produce a total row count.
fn matching_rows(
    tg: &TwinGraph,
    selection: &NodeSelection,
    changed_nodes_only: bool,
    task: &ll::Task,
) -> Result<ResolvedRows> {
    reject_top_k_name_mode(selection)?;

    let mut merged = BTreeSet::new();
    for side in [GraphSide::Left, GraphSide::Right] {
        merged.extend(
            tg.graph(side)
                .select_nodes(selection, &ENUMERATE_ALL, task)?
                .into_iter()
                .map(|local_idx| tg.to_merged(side, local_idx)),
        );
    }

    let total = merged.len();
    let rows: Vec<DeltaRow> = merged
        .into_iter()
        .filter(|&idx| !changed_nodes_only || tg.is_node_changed(idx))
        .map(|idx| node_row(tg, idx))
        .collect();

    Ok(ResolvedRows {
        parent_idx: None,
        hidden_unchanged_count: total - rows.len(),
        rows,
    })
}

fn child_rows(
    tg: &TwinGraph,
    name: &str,
    structure: GraphStructure,
    changed_nodes_only: bool,
) -> Result<(NodeIDX, Vec<DeltaRow>)> {
    let node_idx = find_node(tg, name)?;
    let mut rows: Vec<DeltaRow> = tg
        .get_twin_arrows(node_idx, structure, changed_nodes_only)?
        .into_iter()
        .filter(|arrow| !is_excluded_on_both_sides(arrow))
        .map(|arrow| arrow_row(tg, &arrow))
        .collect();

    // Reverse edges are computed lazily and arrive in non-deterministic order,
    // so the natural (unsorted) output would differ between runs.
    if structure == GraphStructure::Reverse {
        rows.sort_by(|a, b| {
            tg.merged_idx_to_name(a.node_idx)
                .cmp(tg.merged_idx_to_name(b.node_idx))
        });
    }

    Ok((node_idx, rows))
}

// ── Row construction ────────────────────────────────────────────

fn node_row(tg: &TwinGraph, merged_idx: NodeIDX) -> DeltaRow {
    DeltaRow {
        node_idx: merged_idx,
        node_diff: tg.node_diff[merged_idx],
        l: None,
        r: None,
        skipped: 0,
    }
}

/// Turn a [`TwinArrow`] into a row, keeping both sides of the edge.
///
/// The two sides are *not* collapsed into one tag: an edge can be retagged
/// (`lazy` -> `deferred`) or have its dynamic branch changed while both
/// endpoints stay identical, and `node_diff` would record that on the edge's
/// source — not on this row. Keeping `l` and `r` is what makes such a change
/// visible at all.
fn arrow_row(tg: &TwinGraph, arrow: &TwinArrow) -> DeltaRow {
    let left = arrow.l.as_ref();
    let right = arrow.r.as_ref();

    DeltaRow {
        node_idx: arrow.points_to,
        node_diff: tg.node_diff[arrow.points_to],
        l: left.map(edge_of),
        r: right.map(edge_of),
        // In changed-nodes-only mode each side reports its own collapsed path
        // length; the UI shows the shorter one, so we do too.
        skipped: match (left, right) {
            (Some(l), Some(r)) => l.skipped.min(r.skipped),
            (Some(a), None) | (None, Some(a)) => a.skipped,
            (None, None) => 0,
        },
    }
}

fn edge_of(arrow: &unigraph_core::Arrow) -> ExploreDeltaEdge {
    ExploreDeltaEdge {
        tag: arrow.tag.clone(),
        dynamic: arrow.dynamic.clone(),
    }
}

// ── Predicates ──────────────────────────────────────────────────

fn is_reachable_on_either_side(tg: &TwinGraph, merged_idx: NodeIDX) -> bool {
    [GraphSide::Left, GraphSide::Right].into_iter().any(|side| {
        tg.to_local(side, merged_idx)
            .is_some_and(|local_idx| !tg.graph(side).is_node_unreachable(local_idx))
    })
}

/// An edge excluded by traversal on every side it exists on isn't part of the
/// configured graph, so it shouldn't be a row.
fn is_excluded_on_both_sides(arrow: &TwinArrow) -> bool {
    [arrow.l.as_ref(), arrow.r.as_ref()]
        .into_iter()
        .flatten()
        .all(|a| a.excluded)
}

fn find_node(tg: &TwinGraph, name: &str) -> Result<NodeIDX> {
    [GraphSide::Right, GraphSide::Left]
        .into_iter()
        .find_map(|side| {
            tg.graph(side)
                .data
                .node_names_ordered
                .name_to_idx_log(name)
                .map(|local_idx| tg.to_merged(side, local_idx))
        })
        .with_context(|| format!("node '{name}' not found in either graph"))
}

fn no_parent(rows: Vec<DeltaRow>) -> ResolvedRows {
    ResolvedRows {
        parent_idx: None,
        rows,
        hidden_unchanged_count: 0,
    }
}
