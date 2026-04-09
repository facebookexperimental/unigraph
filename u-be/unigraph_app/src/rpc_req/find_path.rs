// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use typegen::TypeGen;
use unigraph_core::ArrayGraph;
use unigraph_core::DynamicEdgeInfo;
use unigraph_core::EdgeMeta;
use unigraph_core::NodeIDX;
use unigraph_core::TraversalType;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_rpc::RpcExec;

use crate::GraphHandle;
use crate::Unigraph;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FindPathInput {
    /// Graph handle — timeline ID, graph key, or GQC key.
    pub handle: String,
    /// Starting node name.
    pub from: String,
    /// Target node name.
    pub to: String,
    /// When true, include a human-readable ASCII summary in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_ascii: Option<bool>,
}

/// A single hop in the path, with edge metadata.
#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct PathHop {
    /// Node name at this position in the path.
    pub node: String,
    /// Edge tag leading *to* this node (e.g. "lazy"). None for the first hop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Dynamic edge info leading *to* this node. None for the first hop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<DynamicEdgeInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TypeGen)]
pub struct FindPathOutput {
    /// The path from `from` to `to`, including edge info per hop.
    /// Empty if no path exists.
    pub path: Vec<PathHop>,
    /// Whether a path was found.
    pub found: bool,
    /// Human-readable summary. Only populated when `include_ascii` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascii: Option<String>,
}

// ── Handler ──────────────────────────────────────────────────

const DEFAULT_TTL_SECS: u64 = 5 * 60;

impl RpcExec<Unigraph> for FindPathInput {
    type Output = FindPathOutput;

    async fn exec(self, ctx: &Unigraph, task: &ll::Task) -> Result<FindPathOutput> {
        let handle: GraphHandle = self.handle.parse()?;
        let ttl = Duration::from_secs(DEFAULT_TTL_SECS);
        let ag = handle.resolve(ctx, task, ttl).await?;
        let input = self;
        task.spawn("find_path", |task| async move {
            tokio::task::spawn_blocking(move || find_path(ag, &input))
                .await
                .context("spawn_blocking panicked")?
        })
        .await
    }
}

// ── Core logic (runs in spawn_blocking) ─────────────────────

fn find_path(ag: Arc<ArrayGraph>, input: &FindPathInput) -> Result<FindPathOutput> {
    let from_idx = resolve_node(&ag, &input.from)?;
    let to_idx = resolve_node(&ag, &input.to)?;

    let path_indices = ag.shortest_path(
        &[from_idx],
        to_idx,
        GraphStructure::Forward,
        TraversalType::Configured,
    );

    let (path, found) = match path_indices {
        Some(indices) => {
            let hops = build_hops(&ag, &indices)?;
            (hops, true)
        }
        None => (Vec::new(), false),
    };

    let ascii = if input.include_ascii.unwrap_or(false) {
        Some(format_ascii(&input.from, &input.to, &path, found))
    } else {
        None
    };

    Ok(FindPathOutput { path, found, ascii })
}

fn resolve_node(ag: &ArrayGraph, name: &str) -> Result<NodeIDX> {
    ag.data
        .node_names_ordered
        .name_to_idx_log(name)
        .with_context(|| format!("node '{}' not found in graph", name))
}

/// Build PathHop entries for each node in the path, looking up edge metadata
/// between consecutive nodes.
fn build_hops(ag: &ArrayGraph, path_indices: &[NodeIDX]) -> Result<Vec<PathHop>> {
    let mut hops = Vec::with_capacity(path_indices.len());

    for (i, &node_idx) in path_indices.iter().enumerate() {
        let name = ag.idx_to_name(node_idx).to_string();

        if i == 0 {
            hops.push(PathHop {
                node: name,
                tag: None,
                dynamic: None,
            });
            continue;
        }

        let prev_idx = path_indices[i - 1];
        let (tag, dynamic) = find_edge_metadata(ag, prev_idx, node_idx);

        hops.push(PathHop {
            node: name,
            tag,
            dynamic,
        });
    }

    Ok(hops)
}

/// Find the edge from `from` to `to` in the forward edge view and extract tag/dynamic info.
fn find_edge_metadata(
    ag: &ArrayGraph,
    from: NodeIDX,
    to: NodeIDX,
) -> (Option<String>, Option<DynamicEdgeInfo>) {
    let view = ag.edge_view(GraphStructure::Forward);
    for (edge, metadata) in view.edges_with_metadata(from) {
        if edge.points_to != to {
            continue;
        }

        return match metadata {
            Some(EdgeMeta::Tagged { tag }) => (Some(tag.clone()), None),
            Some(EdgeMeta::Dynamic {
                type_key,
                edge_name,
                branch,
                ..
            }) => (
                None,
                Some(DynamicEdgeInfo {
                    type_key: type_key.clone(),
                    edge_name: edge_name.clone(),
                    branch: branch.clone(),
                    metadata: None,
                }),
            ),
            _ => (None, None),
        };
    }

    (None, None)
}

// ── ASCII formatting ────────────────────────────────────────

fn format_ascii(from: &str, to: &str, path: &[PathHop], found: bool) -> String {
    let mut out = String::new();

    if !found {
        let _ = write!(out, "No path from \"{from}\" to \"{to}\".");
        return out;
    }

    let steps = path.len() - 1;
    let _ = writeln!(
        out,
        "Shortest path from \"{from}\" to \"{to}\" ({steps} steps):\n"
    );

    for (i, hop) in path.iter().enumerate() {
        let _ = write!(out, "{}", hop.node);
        if i < path.len() - 1 {
            // Edge annotation belongs to the next hop (the edge leading to it).
            let edge_annotation = format_edge_annotation(&path[i + 1]);
            let _ = writeln!(out, "{edge_annotation} ->");
        }
    }
    let _ = writeln!(out);

    out
}

/// Format the edge annotation for a hop: ` [tag: lazy]` or ` [platform:button/ios]`
fn format_edge_annotation(hop: &PathHop) -> String {
    if let Some(tag) = &hop.tag {
        return format!(" [tag: {tag}]");
    }
    if let Some(d) = &hop.dynamic {
        return format!(" [{}:{}/{}]", d.type_key, d.edge_name, d.branch);
    }
    String::new()
}
