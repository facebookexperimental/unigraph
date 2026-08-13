// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Condense a graph into its DAG of strongly connected components (SCCs).
//!
//! Cycles in a dependency graph make it hard to see the "big picture" — you
//! can't tell which clusters depend on which. This module collapses every SCC
//! into a single node, producing a [`MapGraph`] that is a tree/DAG of clusters.
//!
//! ```text
//!   original (has a cycle B<->C)          condensed
//!        A                                    A
//!       / \                                  / \
//!      B   D          ==>                "SCC #1"  D
//!      |\ /                              (= {B, C})
//!      | C
//!      |/
//!      E                                    E
//! ```
//!
//! Rules:
//! - **Singleton SCC** (one node): kept as-is — same name, metrics, labels,
//!   properties. Only its outgoing edges are patched (see below).
//! - **Multi-node SCC**: replaced by a synthetic node named `"SCC #N"` (1-based,
//!   incrementing). Its metrics are the element-wise sum of its members' metrics,
//!   plus a [`SCC_NODE_COUNT_METRIC`] metric holding the member count.
//! - **Edge into a multi-node SCC**: becomes a dynamic edge on the source node
//!   with type [`SCC_DYNAMIC_TYPE`] (`"scc"`), edge name [`SCC_EDGE_NAME`]
//!   (`"in"`), and one branch per entered component (the original name of the
//!   member node the edge lands on). This records *how* the cluster is entered.
//! - **Edge into a singleton**: keeps its original type (directed / tagged /
//!   dynamic) and metadata.
//! - **Internal edges** (both endpoints in the same SCC): dropped.
//!
//! Only reachable nodes participate — [`ArrayGraph::sccs`] skips unreachable
//! ones and follows only non-excluded edges (honoring the active traversal
//! config).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::MapGraph;
use crate::NodeIDX;
use crate::types::array_graph::offset_graph::edge_flags::EdgeFlags;
use crate::types::map_graph::DynamicEdge;
use crate::types::map_graph::GraphNode;

/// Dynamic-edge type key for synthesized edges pointing into a multi-node SCC.
const SCC_DYNAMIC_TYPE: &str = "scc";
/// Dynamic-edge name for synthesized edges pointing into a multi-node SCC.
const SCC_EDGE_NAME: &str = "in";
/// Metric holding the number of nodes collapsed into a multi-node SCC.
const SCC_NODE_COUNT_METRIC: &str = "scc_node_count";
/// Prefix for synthetic multi-node SCC names, e.g. `"SCC #1"`.
const SCC_NAME_PREFIX: &str = "SCC #";

/// Result of condensing a graph into its SCC DAG.
pub struct SccGraph {
    /// The condensed graph: one node per SCC.
    pub graph: MapGraph,
    /// Members of each *multi-node* SCC: its `"SCC #N"` name → original node names.
    pub members: BTreeMap<String, BTreeSet<String>>,
}

/// Condense `graph` into its DAG of strongly connected components.
pub fn to_scc_graph(graph: &ArrayGraph) -> Result<SccGraph> {
    let ctx = SccContext::build(graph);
    let nodes = ctx.build_condensed_nodes();
    let members = ctx.collect_multi_node_members();
    let entry_points = ctx.condensed_entry_points();

    let map_graph = MapGraph {
        nodes,
        traversal_config: None,
        graph_settings: None,
        entry_points,
        properties: BTreeMap::new(),
    };

    Ok(SccGraph {
        graph: map_graph,
        members,
    })
}

// -- Implementation -----------------------------------------------------------

/// Precomputed lookups shared across the condensation steps.
struct SccContext<'a> {
    graph: &'a ArrayGraph,
    sccs: &'a Vec<Vec<NodeIDX>>,
    /// NodeIDX → its SCC index (None for unreachable nodes not in any SCC).
    node_scc: Vec<Option<usize>>,
    /// SCC index → its node name in the condensed graph.
    scc_name: Vec<String>,
    /// SCC index → whether it collapses more than one node.
    scc_is_multi: Vec<bool>,
}

impl<'a> SccContext<'a> {
    fn build(graph: &'a ArrayGraph) -> Self {
        let sccs = graph.sccs();
        let mut node_scc: Vec<Option<usize>> = vec![None; graph.nodes_len()];
        let mut scc_name = Vec::with_capacity(sccs.len());
        let mut scc_is_multi = Vec::with_capacity(sccs.len());
        let mut multi_counter = 0usize;

        for (scc_idx, scc) in sccs.iter().enumerate() {
            for &node_idx in scc {
                node_scc[node_idx] = Some(scc_idx);
            }
            let is_multi = scc.len() > 1;
            scc_is_multi.push(is_multi);
            if is_multi {
                multi_counter += 1;
                scc_name.push(format!("{SCC_NAME_PREFIX}{multi_counter}"));
            } else {
                scc_name.push(graph.idx_to_name(scc[0]).to_string());
            }
        }

        SccContext {
            graph,
            sccs,
            node_scc,
            scc_name,
            scc_is_multi,
        }
    }

    fn build_condensed_nodes(&self) -> BTreeMap<String, GraphNode> {
        let mut nodes = BTreeMap::new();
        for (scc_idx, scc) in self.sccs.iter().enumerate() {
            let node = self.build_condensed_node(scc_idx, scc);
            nodes.insert(self.scc_name[scc_idx].clone(), node);
        }
        nodes
    }

    fn build_condensed_node(&self, scc_idx: usize, scc: &[NodeIDX]) -> GraphNode {
        let edges = self.collect_edges(scc_idx, scc);
        if scc.len() == 1 {
            self.singleton_node(scc[0], edges)
        } else {
            self.multi_node(scc, edges)
        }
    }

    /// A singleton keeps its original metrics/labels/properties; only its
    /// outgoing edges are replaced with the condensed (patched) ones.
    fn singleton_node(&self, node_idx: NodeIDX, edges: EdgeAccum) -> GraphNode {
        let mut node = self.graph.get_map_node(node_idx);
        node.edges_directed = none_if_empty_set(edges.directed);
        node.edges_tagged = none_if_empty_map(edges.tagged);
        node.edges_dynamic = none_if_empty_map(edges.dynamic);
        node
    }

    fn multi_node(&self, scc: &[NodeIDX], edges: EdgeAccum) -> GraphNode {
        GraphNode {
            properties: None,
            labels: None,
            metrics: Some(self.summed_metrics(scc)),
            edges_directed: none_if_empty_set(edges.directed),
            edges_tagged: none_if_empty_map(edges.tagged),
            edges_dynamic: none_if_empty_map(edges.dynamic),
        }
    }

    /// Element-wise sum of every metric across the SCC's members, plus the
    /// synthetic `scc_node_count` metric.
    fn summed_metrics(&self, scc: &[NodeIDX]) -> BTreeMap<String, f64> {
        let mut out = BTreeMap::new();
        for (name, values) in &self.graph.data.node_metadata.metrics {
            let mut sum = 0.0f64;
            for &node_idx in scc {
                sum += values[node_idx];
            }
            if sum != 0.0 {
                out.insert(name.clone(), sum);
            }
        }
        out.insert(SCC_NODE_COUNT_METRIC.to_string(), scc.len() as f64);
        out
    }

    fn collect_edges(&self, scc_idx: usize, scc: &[NodeIDX]) -> EdgeAccum {
        let mut accum = EdgeAccum::new();
        for &node_idx in scc {
            self.route_directed(&mut accum, scc_idx, node_idx);
            self.route_tagged(&mut accum, scc_idx, node_idx);
            self.route_dynamic(&mut accum, scc_idx, node_idx);
        }
        accum
    }

    fn route_directed(&self, accum: &mut EdgeAccum, source_scc: usize, node_idx: NodeIDX) {
        for (target, flags) in self.graph.forward_edges(node_idx) {
            if flags.contains(EdgeFlags::EXCLUDED) {
                continue;
            }
            // Tagged/dynamic edges are handled by their own routers below.
            if flags.intersects(EdgeFlags::IS_TAGGED | EdgeFlags::IS_DYNAMIC) {
                continue;
            }
            self.route_target(accum, source_scc, target, |accum, target_name| {
                accum.directed.insert(target_name.to_string());
            });
        }
    }

    fn route_tagged(&self, accum: &mut EdgeAccum, source_scc: usize, node_idx: NodeIDX) {
        for (tag, targets) in self.graph.data.edges.tagged_edges_for_node(node_idx) {
            for target in targets {
                self.route_target(accum, source_scc, target, |accum, target_name| {
                    accum.add_tagged(tag, target_name);
                });
            }
        }
    }

    fn route_dynamic(&self, accum: &mut EdgeAccum, source_scc: usize, node_idx: NodeIDX) {
        for (type_key, edge_map) in self.graph.data.edges.dynamic_edges_for_node(node_idx) {
            for (edge_name, view) in edge_map {
                let metadata = view.metadata;
                for (branch, targets) in view.branches {
                    for target in targets {
                        self.route_target(accum, source_scc, target, |accum, target_name| {
                            accum.add_dynamic(type_key, edge_name, branch, metadata, target_name);
                        });
                    }
                }
            }
        }
    }

    /// Route one edge from `source_scc` to `target` into the accumulator.
    ///
    /// Drops internal edges. Edges into a multi-node SCC become the synthetic
    /// `scc`/`in` dynamic edge (branch = entered component). Edges into a
    /// singleton fall through to `on_singleton`, which preserves the original
    /// edge type.
    fn route_target(
        &self,
        accum: &mut EdgeAccum,
        source_scc: usize,
        target: NodeIDX,
        on_singleton: impl FnOnce(&mut EdgeAccum, &str),
    ) {
        let target_scc = match self.node_scc[target] {
            Some(scc_idx) => scc_idx,
            None => return,
        };
        if target_scc == source_scc {
            return;
        }
        let target_name = &self.scc_name[target_scc];
        if self.scc_is_multi[target_scc] {
            let entered_component = self.graph.idx_to_name(target);
            accum.add_scc_branch(entered_component, target_name);
        } else {
            on_singleton(accum, target_name);
        }
    }

    fn collect_multi_node_members(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut members = BTreeMap::new();
        for (scc_idx, scc) in self.sccs.iter().enumerate() {
            if !self.scc_is_multi[scc_idx] {
                continue;
            }
            let names = scc
                .iter()
                .map(|&node_idx| self.graph.idx_to_name(node_idx).to_string())
                .collect();
            members.insert(self.scc_name[scc_idx].clone(), names);
        }
        members
    }

    /// The condensed graph's entry points: the original entry points mapped to
    /// the SCC nodes that contain them.
    fn condensed_entry_points(&self) -> Option<BTreeSet<String>> {
        let entries: BTreeSet<String> = self
            .graph
            .determine_entrypoints()
            .iter()
            .filter_map(|&node_idx| {
                self.node_scc[node_idx].map(|scc_idx| self.scc_name[scc_idx].clone())
            })
            .collect();
        if entries.is_empty() {
            None
        } else {
            Some(entries)
        }
    }
}

/// Accumulates the condensed outgoing edges of a single SCC node.
struct EdgeAccum {
    directed: BTreeSet<String>,
    tagged: BTreeMap<String, BTreeSet<String>>,
    dynamic: BTreeMap<String, BTreeMap<String, DynamicEdge>>,
}

impl EdgeAccum {
    fn new() -> Self {
        EdgeAccum {
            directed: BTreeSet::new(),
            tagged: BTreeMap::new(),
            dynamic: BTreeMap::new(),
        }
    }

    fn add_tagged(&mut self, tag: &str, target: &str) {
        self.tagged
            .entry(tag.to_string())
            .or_default()
            .insert(target.to_string());
    }

    fn add_dynamic(
        &mut self,
        type_key: &str,
        edge_name: &str,
        branch: &str,
        metadata: Option<&BTreeMap<String, String>>,
        target: &str,
    ) {
        self.dynamic
            .entry(type_key.to_string())
            .or_default()
            .entry(edge_name.to_string())
            .or_insert_with(|| DynamicEdge {
                branches: BTreeMap::new(),
                metadata: metadata.cloned(),
            })
            .branches
            .entry(branch.to_string())
            .or_default()
            .insert(target.to_string());
    }

    /// Add a synthetic `scc`/`in` branch recording that `target` (a multi-node
    /// SCC) is entered via `entered_component`.
    fn add_scc_branch(&mut self, entered_component: &str, target: &str) {
        self.dynamic
            .entry(SCC_DYNAMIC_TYPE.to_string())
            .or_default()
            .entry(SCC_EDGE_NAME.to_string())
            .or_insert_with(|| DynamicEdge {
                branches: BTreeMap::new(),
                metadata: None,
            })
            .branches
            .entry(entered_component.to_string())
            .or_default()
            .insert(target.to_string());
    }
}

fn none_if_empty_set(set: BTreeSet<String>) -> Option<BTreeSet<String>> {
    if set.is_empty() { None } else { Some(set) }
}

fn none_if_empty_map<V>(map: BTreeMap<String, V>) -> Option<BTreeMap<String, V>> {
    if map.is_empty() { None } else { Some(map) }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::tests::test_graphs::make_test_array_graph_2;

    #[test]
    fn test_to_scc_graph() -> Result<()> {
        let ag = make_test_array_graph_2()?;
        let scc = to_scc_graph(&ag)?;

        let members_json = serde_json::to_string_pretty(&scc.members)?;
        let graph_json = serde_json::to_string_pretty(&scc.graph)?;

        snapshot!(
            members_json,
            r#"
{
  "SCC #1": [
    "M",
    "N",
    "O"
  ]
}
"#
        );
        snapshot!(
            graph_json,
            r#"
{
  "nodes": {
    "A": {
      "metrics": {
        "size": 1.0
      },
      "edges_directed": [
        "B",
        "D"
      ]
    },
    "B": {
      "metrics": {
        "size": 1.0
      },
      "edges_tagged": {
        "BL": [
          "C"
        ],
        "RD": [
          "J"
        ]
      }
    },
    "C": {
      "labels": {
        "disallow_tags": [
          "b",
          "c"
        ]
      },
      "metrics": {
        "size": 1.0
      }
    },
    "D": {
      "metrics": {
        "size": 1.0
      },
      "edges_directed": [
        "F"
      ],
      "edges_tagged": {
        "RDFD": [
          "E"
        ]
      }
    },
    "E": {
      "metrics": {
        "size": 1.0
      },
      "edges_directed": [
        "K"
      ]
    },
    "F": {
      "metrics": {
        "size": 1.0
      },
      "edges_dynamic": {
        "ddd": {
          "ddd_1": {
            "branches": {
              "b1": [
                "G",
                "H"
              ],
              "b2": [
                "I"
              ]
            }
          }
        }
      }
    },
    "G": {
      "metrics": {
        "size": 1.0
      }
    },
    "H": {
      "metrics": {
        "size": 1.0
      }
    },
    "I": {
      "metrics": {
        "size": 1.0
      }
    },
    "J": {
      "labels": {
        "assert_tags": [
          "a",
          "b"
        ]
      },
      "metrics": {
        "size": 1.0
      },
      "edges_directed": [
        "K"
      ]
    },
    "K": {
      "metrics": {
        "size": 1.0
      }
    },
    "L": {
      "metrics": {
        "size": 1.0
      },
      "edges_directed": [
        "D"
      ],
      "edges_dynamic": {
        "scc": {
          "in": {
            "branches": {
              "M": [
                "SCC #1"
              ]
            }
          }
        }
      }
    },
    "P": {
      "metrics": {
        "size": 1.0
      }
    },
    "SCC #1": {
      "metrics": {
        "scc_node_count": 3.0,
        "size": 3.0
      },
      "edges_directed": [
        "P"
      ],
      "edges_tagged": {
        "BL": [
          "F"
        ]
      }
    }
  },
  "traversal_config": null,
  "graph_settings": null,
  "entry_points": [
    "A",
    "L"
  ]
}
"#
        );
        Ok(())
    }
}
