// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Narrowing the tree table's flat list down to the nodes matching a filter.
//!
//! Backs [`ArrayGraphUISettingsTreeTableEntryPoints::Filtered`]. A node has to
//! satisfy every condition in the filter — properties, incoming edges and
//! outgoing edges are ANDed together, as are the entries within each.
//!
//! The passes are ordered so the expensive ones run on the smallest set:
//!
//! ```text
//!   properties  ->  intersect inverted indices, seeded from the smallest
//!        |          (whole reachable graph when no properties are asked for)
//!        v
//!   reachable   ->  drop nodes the traversal config pruned
//!        |
//!        v
//!   incoming    ->  O(in-degree) scan of the reverse graph per candidate
//!   edges
//!        |
//!        v
//!   outgoing    ->  O(out-degree) scan of the forward graph per candidate
//!   edges
//! ```
//!
//! Each edge pass is skipped entirely when that direction has no conditions, so
//! a properties-only filter never builds the reverse graph.
//!
//! Nothing is cached here. The result is memoized on the frontend, keyed on the
//! filter, which also covers traversal-config changes — see the module docs on
//! graph settings for why a cache on this side would go stale.

use crate::EdgeMeta;
use crate::NodeIDX;
use crate::graph_settings::EdgeConditions;
use crate::graph_settings::EntryPointsFilter;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::ArrayGraph;
use crate::types::array_graph::offset_graph::EdgeGraphView;
use crate::types::array_graph::property_index::PropertyIndices;

/// Every reachable node matching `filter`, ascending.
pub fn filter_entry_points(ag: &ArrayGraph, filter: &EntryPointsFilter) -> Vec<NodeIDX> {
    let Some(candidates) = reachable_candidates_from_properties(ag, filter) else {
        return Vec::new();
    };
    let candidates = retain_by_edges(
        ag,
        candidates,
        GraphStructure::Reverse,
        filter.incoming_edges(),
    );
    retain_by_edges(
        ag,
        candidates,
        GraphStructure::Forward,
        filter.outgoing_edges(),
    )
}

/// Reachable nodes matching every property condition, ascending.
///
/// `None` when a requested property name is absent from the graph: that
/// condition can never hold, so the whole filter is unsatisfiable.
fn reachable_candidates_from_properties(
    ag: &ArrayGraph,
    filter: &EntryPointsFilter,
) -> Option<Vec<NodeIDX>> {
    let conditions = filter
        .properties
        .iter()
        .map(|(name, value_match)| (name.as_str(), value_match.value.as_deref()));
    let indices = PropertyIndices::bind(ag, conditions)?;

    if indices.is_empty() {
        return Some(ag.all_reachable_node_idxs());
    }

    Some(
        indices
            .intersect()
            .into_iter()
            .filter(|&node_idx| !ag.is_node_unreachable(node_idx))
            .collect(),
    )
}

/// Drop candidates whose edges in `structure` don't carry every requested tag
/// and dynamic type key.
///
/// Returns the candidates untouched when this direction has no conditions, so
/// the edge view — the reverse graph in particular — is never built for nothing.
fn retain_by_edges(
    ag: &ArrayGraph,
    mut candidates: Vec<NodeIDX>,
    structure: GraphStructure,
    conditions: EdgeConditions<'_>,
) -> Vec<NodeIDX> {
    if conditions.is_empty() {
        return candidates;
    }

    let view = ag.edge_view(structure);
    candidates.retain(|&node_idx| matches_edges(&view, node_idx, &conditions));
    candidates
}

fn matches_edges(
    view: &EdgeGraphView<'_>,
    node_idx: NodeIDX,
    conditions: &EdgeConditions<'_>,
) -> bool {
    conditions
        .tags
        .iter()
        .all(|tag| has_tagged_edge(view, node_idx, tag))
        && conditions
            .dynamic_type_keys
            .iter()
            .all(|type_key| has_dynamic_edge(view, node_idx, type_key))
}

fn has_tagged_edge(view: &EdgeGraphView<'_>, node_idx: NodeIDX, tag: &str) -> bool {
    edge_metadata(view, node_idx)
        .any(|meta| matches!(meta, EdgeMeta::Tagged { tag: actual } if actual == tag))
}

fn has_dynamic_edge(view: &EdgeGraphView<'_>, node_idx: NodeIDX, type_key: &str) -> bool {
    edge_metadata(view, node_idx).any(
        |meta| matches!(meta, EdgeMeta::Dynamic { type_key: actual, .. } if actual == type_key),
    )
}

/// Metadata of the node's tagged and dynamic edges in this view.
///
/// Edges the traversal config excluded are skipped: the filter describes the
/// graph as currently configured, matching the fact that we only ever consider
/// reachable nodes. Plain directed edges carry no metadata and are skipped too.
fn edge_metadata<'a>(
    view: &'a EdgeGraphView<'a>,
    node_idx: NodeIDX,
) -> impl Iterator<Item = &'a EdgeMeta> + 'a {
    view.edges_with_metadata(node_idx)
        .filter(|(edge, _)| !edge.is_excluded())
        .filter_map(|(_, meta)| meta)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::snapshot;

    use super::*;
    use crate::MapGraph;
    use crate::TraversalConfig;
    use crate::graph_settings::PropertyValueMatch;
    use crate::types::array_graph::filter_candidates::filter_candidates;

    /// ```text
    ///                    root
    ///        directed  /   |  \  dynamic rc:gk/gate_1
    ///                 /    |   \      on -> delta
    ///          alpha epsilon    \     off -> zeta
    ///                           |
    ///              tagged lazy -+-> beta, gamma, delta
    ///             tagged eager -+-> epsilon
    ///
    ///   beta  --tagged lazy-->  gamma
    ///   gamma --tagged eager--> epsilon
    ///   delta --dynamic rc:gk/gate_2--> zeta
    /// ```
    ///
    /// `delta` is deliberately reachable by both a tagged and a dynamic edge so
    /// the AND across edge kinds has something to bite on, and `zeta` carries
    /// `budget_type=PAGE` so "has the property" differs from "equals ROUTE".
    ///
    /// The three non-root edges exist so outgoing conditions have more than
    /// `root` to match, and are chosen to duplicate an edge kind their target
    /// already had incoming — so they leave every incoming result untouched.
    const TEST_GRAPH: &str = r#"{
      "nodes": {
        "root": {
          "edges_directed": ["alpha", "epsilon"],
          "edges_tagged": { "lazy": ["beta", "gamma", "delta"], "eager": ["epsilon"] },
          "edges_dynamic": {
            "rc:gk": { "gate_1": { "branches": { "on": ["delta"], "off": ["zeta"] } } }
          }
        },
        "alpha":   { "properties": { "budget_type": "ROUTE", "team": "ads" } },
        "beta":    {
          "properties": { "budget_type": "ROUTE" },
          "edges_tagged": { "lazy": ["gamma"] }
        },
        "gamma":   {
          "properties": { "budget_type": "PAGE", "team": "ads" },
          "edges_tagged": { "eager": ["epsilon"] }
        },
        "delta":   {
          "properties": { "budget_type": "ROUTE", "team": "ads" },
          "edges_dynamic": {
            "rc:gk": { "gate_2": { "branches": { "on": ["zeta"], "off": [] } } }
          }
        },
        "epsilon": { "properties": { "team": "core" } },
        "zeta":    { "properties": { "budget_type": "PAGE" } }
      },
      "traversal_config": null,
      "graph_settings": null,
      "entry_points": null
    }"#;

    fn test_graph() -> Result<ArrayGraph> {
        MapGraph::from_json(TEST_GRAPH)?.to_array_graph(&ll::Task::create_new("test"))
    }

    /// Builder so each case below stays a one-liner. An inherent impl is legal
    /// here because it's the same crate the type is defined in.
    impl EntryPointsFilter {
        fn prop(mut self, name: &str, value: Option<&str>) -> Self {
            self.properties.insert(
                name.to_string(),
                PropertyValueMatch {
                    value: value.map(str::to_string),
                },
            );
            self
        }

        fn in_tag(mut self, tag: &str) -> Self {
            self.incoming_tags.insert(tag.to_string());
            self
        }

        fn in_dyn(mut self, type_key: &str) -> Self {
            self.incoming_dynamic_type_keys.insert(type_key.to_string());
            self
        }

        fn out_tag(mut self, tag: &str) -> Self {
            self.outgoing_tags.insert(tag.to_string());
            self
        }

        fn out_dyn(mut self, type_key: &str) -> Self {
            self.outgoing_dynamic_type_keys.insert(type_key.to_string());
            self
        }
    }

    fn filter() -> EntryPointsFilter {
        EntryPointsFilter::default()
    }

    fn all_cases() -> Vec<(&'static str, EntryPointsFilter)> {
        vec![
            ("no conditions", filter()),
            (
                "budget_type=ROUTE",
                filter().prop("budget_type", Some("ROUTE")),
            ),
            (
                "budget_type=ROUTE + team=ads",
                filter()
                    .prop("budget_type", Some("ROUTE"))
                    .prop("team", Some("ads")),
            ),
            ("has budget_type", filter().prop("budget_type", None)),
            ("team=core", filter().prop("team", Some("core"))),
            ("incoming tag lazy", filter().in_tag("lazy")),
            ("incoming tag eager", filter().in_tag("eager")),
            ("incoming dynamic rc:gk", filter().in_dyn("rc:gk")),
            (
                "incoming tag lazy + dynamic rc:gk",
                filter().in_tag("lazy").in_dyn("rc:gk"),
            ),
            (
                "incoming tag lazy + eager",
                filter().in_tag("lazy").in_tag("eager"),
            ),
            (
                "budget_type=ROUTE + incoming tag lazy",
                filter().prop("budget_type", Some("ROUTE")).in_tag("lazy"),
            ),
            (
                "budget_type=PAGE + incoming tag lazy",
                filter().prop("budget_type", Some("PAGE")).in_tag("lazy"),
            ),
            ("outgoing tag lazy", filter().out_tag("lazy")),
            ("outgoing tag eager", filter().out_tag("eager")),
            ("outgoing dynamic rc:gk", filter().out_dyn("rc:gk")),
            (
                "outgoing tag lazy + eager",
                filter().out_tag("lazy").out_tag("eager"),
            ),
            (
                "outgoing tag lazy + dynamic rc:gk",
                filter().out_tag("lazy").out_dyn("rc:gk"),
            ),
            (
                "budget_type=ROUTE + outgoing tag lazy",
                filter().prop("budget_type", Some("ROUTE")).out_tag("lazy"),
            ),
            (
                "incoming tag lazy + outgoing tag lazy",
                filter().in_tag("lazy").out_tag("lazy"),
            ),
            (
                "incoming tag lazy + outgoing tag eager",
                filter().in_tag("lazy").out_tag("eager"),
            ),
            (
                "incoming dynamic rc:gk + outgoing dynamic rc:gk",
                filter().in_dyn("rc:gk").out_dyn("rc:gk"),
            ),
            (
                "incoming tag eager + outgoing tag lazy",
                filter().in_tag("eager").out_tag("lazy"),
            ),
            ("unknown property name", filter().prop("nope", Some("x"))),
            (
                "known name, unknown value",
                filter().prop("budget_type", Some("NOPE")),
            ),
            ("unknown tag", filter().in_tag("nope")),
            ("unknown dynamic type", filter().in_dyn("nope")),
            (
                "known property + unknown tag",
                filter().prop("budget_type", Some("ROUTE")).in_tag("nope"),
            ),
            ("unknown outgoing tag", filter().out_tag("nope")),
            ("unknown outgoing dynamic type", filter().out_dyn("nope")),
        ]
    }

    fn render_cases(ag: &ArrayGraph, cases: &[(&str, EntryPointsFilter)]) -> String {
        let rows: Vec<(&str, String)> = cases
            .iter()
            .map(|(label, filter)| {
                let matches = filter_entry_points(ag, filter);
                let names = ag.idxs_to_names(&matches).join(", ");
                (
                    *label,
                    if names.is_empty() {
                        "(none)".to_string()
                    } else {
                        names
                    },
                )
            })
            .collect();

        let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
        rows.iter()
            .map(|(label, names)| format!("{label:<width$}  |  {names}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_filter_entry_points() -> Result<()> {
        let ag = test_graph()?;
        snapshot!(
            render_cases(&ag, &all_cases()),
            "
no conditions                                    |  alpha, beta, delta, epsilon, gamma, root, zeta
budget_type=ROUTE                                |  alpha, beta, delta
budget_type=ROUTE + team=ads                     |  alpha, delta
has budget_type                                  |  alpha, beta, delta, gamma, zeta
team=core                                        |  epsilon
incoming tag lazy                                |  beta, delta, gamma
incoming tag eager                               |  epsilon
incoming dynamic rc:gk                           |  delta, zeta
incoming tag lazy + dynamic rc:gk                |  delta
incoming tag lazy + eager                        |  (none)
budget_type=ROUTE + incoming tag lazy            |  beta, delta
budget_type=PAGE + incoming tag lazy             |  gamma
outgoing tag lazy                                |  beta, root
outgoing tag eager                               |  gamma, root
outgoing dynamic rc:gk                           |  delta, root
outgoing tag lazy + eager                        |  root
outgoing tag lazy + dynamic rc:gk                |  root
budget_type=ROUTE + outgoing tag lazy            |  beta
incoming tag lazy + outgoing tag lazy            |  beta
incoming tag lazy + outgoing tag eager           |  gamma
incoming dynamic rc:gk + outgoing dynamic rc:gk  |  delta
incoming tag eager + outgoing tag lazy           |  (none)
unknown property name                            |  (none)
known name, unknown value                        |  (none)
unknown tag                                      |  (none)
unknown dynamic type                             |  (none)
known property + unknown tag                     |  (none)
unknown outgoing tag                             |  (none)
unknown outgoing dynamic type                    |  (none)
"
        );
        Ok(())
    }

    /// A node the traversal config pruned must never show up, even when it
    /// still matches every condition on paper.
    #[test]
    fn test_filter_excludes_unreachable_nodes() -> Result<()> {
        let mut ag = test_graph()?;
        let tvc: TraversalConfig =
            serde_json::from_str(r#"{"force_nodes": {"delta": {"include": false}}}"#)?;
        ag.apply_traversal_config_and_entry_points(tvc)?;

        let cases = [
            (
                "budget_type=ROUTE",
                filter().prop("budget_type", Some("ROUTE")),
            ),
            ("incoming tag lazy", filter().in_tag("lazy")),
            ("incoming dynamic rc:gk", filter().in_dyn("rc:gk")),
            ("outgoing dynamic rc:gk", filter().out_dyn("rc:gk")),
        ];

        snapshot!(
            render_cases(&ag, &cases),
            "
budget_type=ROUTE       |  alpha, beta
incoming tag lazy       |  beta, gamma
incoming dynamic rc:gk  |  zeta
outgoing dynamic rc:gk  |  root
"
        );
        Ok(())
    }

    /// An excluded edge must not satisfy an edge condition in either direction,
    /// even when the nodes at both of its ends are still reachable some other
    /// way. `epsilon` keeps its directed edge from `root` and `gamma` its lazy
    /// edge, so this isolates edge exclusion from the node-level reachability
    /// check above.
    #[test]
    fn test_filter_ignores_excluded_edges() -> Result<()> {
        let mut ag = test_graph()?;
        let tvc: TraversalConfig =
            serde_json::from_str(r#"{"force_tagged": {"eager": {"include": false}}}"#)?;
        ag.apply_traversal_config_and_entry_points(tvc)?;

        let cases = [
            ("incoming tag eager", filter().in_tag("eager")),
            ("outgoing tag eager", filter().out_tag("eager")),
            ("outgoing tag lazy", filter().out_tag("lazy")),
            ("team=core", filter().prop("team", Some("core"))),
        ];

        snapshot!(
            render_cases(&ag, &cases),
            "
incoming tag eager  |  (none)
outgoing tag eager  |  (none)
outgoing tag lazy   |  beta, root
team=core           |  epsilon
"
        );
        Ok(())
    }

    #[test]
    fn test_filter_candidates() -> Result<()> {
        let ag = test_graph()?;
        let candidates = filter_candidates(&ag);
        snapshot!(
            serde_json::to_string_pretty(&candidates)?,
            r#"
{
  "properties": [
    {
      "name": "budget_type",
      "values": [
        "PAGE",
        "ROUTE"
      ],
      "high_cardinality": false
    },
    {
      "name": "team",
      "values": [
        "ads",
        "core"
      ],
      "high_cardinality": false
    }
  ],
  "tags": [
    "eager",
    "lazy"
  ],
  "dynamic_type_keys": [
    "rc:gk"
  ]
}
"#
        );
        Ok(())
    }
}
