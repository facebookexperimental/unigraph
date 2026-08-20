// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Evaluating a [`NodeSelection`] against a graph.
//!
//! Backs the tree table's [`ArrayGraphUISettingsTreeTableEntryPoints::Filtered`]
//! flat list, the `SearchNodes` RPC, and the `Matching` target of
//! `ExploreGraph` / `ExploreDelta`. A node has to satisfy every condition in the
//! selection — name, properties, incoming edges and outgoing edges are ANDed
//! together, as are the entries within each.
//!
//! # Seed, then retain
//!
//! One pass *seeds* the candidate set and the rest *retain* from it, ordered so
//! the expensive passes run on the smallest set:
//!
//! ```text
//!   seed      ->  Exact:            one binary search on the name list
//!                 Fuzzy:            top-K subsequence search, capped
//!                 otherwise:        intersect the property indices, seeded
//!                                   from the smallest (all nodes when no
//!                                   properties are asked for)
//!        |
//!        v
//!   properties ->  only when the name seeded instead
//!        |
//!        v
//!   reachable  ->  drop nodes the traversal config pruned (opt-in)
//!        |
//!        v
//!   name       ->  Substring/Regex only: one compiled regex per candidate name
//!        |
//!        v
//!   incoming   ->  O(in-degree) scan of the reverse graph per candidate
//!   edges
//!        |
//!        v
//!   outgoing   ->  O(out-degree) scan of the forward graph per candidate
//!   edges
//! ```
//!
//! `Exact` and `Fuzzy` seed because they *generate* candidates straight out of
//! the name list; `Substring` and `Regex` can only *test* a name, so they stay a
//! retain pass and run before the edge passes — they need no graph structure, so
//! they shrink the set for the cost of one string test per node. Every pass is
//! skipped when its condition is absent, so a properties-only selection never
//! builds the reverse graph and never compiles a regex.
//!
//! # Ordering
//!
//! Results come back ascending by [`NodeIDX`] — except under `Fuzzy`, which is
//! shortest-name-first because that is what makes a typeahead useful. Callers
//! wanting another order sort afterwards.
//!
//! # Fuzzy is top-K, and that leaks
//!
//! `Fuzzy` is bounded by construction, so it always runs against a cap
//! ([`SelectOptions::limit`], else [`DEFAULT_FUZZY_CAP`]) and returns a
//! *prefix* of the real match set rather than all of it. Two consequences:
//!
//! - A count taken from a fuzzy result is not the number of matching nodes, so
//!   a caller that needs a true total — pagination with a row count, say —
//!   must not offer this mode. `ExploreGraph` rejects it for exactly that
//!   reason; `SearchNodes` allows it, because a typeahead wants top-K and
//!   treats its limit as the whole contract.
//! - The property and edge passes run *after* the top-K, so a fuzzy selection
//!   with other conditions can return fewer than `limit` even when more nodes
//!   would match. Widening the cap to compensate would mean scanning the whole
//!   name list on every keystroke, which is exactly what the top-K exists to
//!   avoid.
//!
//! On a multi-million-node graph the `Substring`/`Regex` name pass is a full
//! scan, which is why the UI debounces the pattern instead of committing it per
//! keystroke.
//!
//! Nothing is cached here. The result is memoized on the frontend, keyed on the
//! selection, which also covers traversal-config changes — see the module docs
//! on graph settings for why a cache on this side would go stale.

use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use regex::RegexBuilder;

use crate::EdgeMeta;
use crate::NodeIDX;
use crate::graph_settings::GraphStructure;
use crate::types::array_graph::ArrayGraph;
use crate::types::array_graph::node_selection::EdgeConditions;
use crate::types::array_graph::node_selection::NameMatch;
use crate::types::array_graph::node_selection::NameMatchMode;
use crate::types::array_graph::node_selection::NodeSelection;
use crate::types::array_graph::offset_graph::EdgeGraphView;
use crate::types::array_graph::property_index::PropertyIndices;

/// Cap applied to `Fuzzy` when the caller didn't ask for one.
///
/// `search_name_fuzzy` sizes a `BinaryHeap` per rayon thread from this, so it
/// has to stay a number and can never be `usize::MAX`.
pub const DEFAULT_FUZZY_CAP: usize = 1_000;

/// Evaluator knobs. Deliberately not part of [`NodeSelection`] — these are
/// call-site concerns, not something worth persisting or sending over the wire.
#[derive(Debug, Clone, Copy)]
pub struct SelectOptions {
    /// Cap on results. Always applied to `Fuzzy`, which is top-K by
    /// construction; `None` on the other modes returns every match.
    pub limit: Option<usize>,
    /// Drop nodes the traversal config pruned. The tree table and explore want
    /// this; a raw search over node names does not.
    pub reachable_only: bool,
}

/// Every node matching `selection`, ascending — except under `Fuzzy`, which is
/// shortest-name-first and capped. See the module docs.
///
/// Fails only on an unparseable name regex — the UI validates the pattern as
/// it's typed, so reaching here with a bad one means something skipped that.
pub fn select_nodes(
    ag: &ArrayGraph,
    selection: &NodeSelection,
    opts: &SelectOptions,
    task: &ll::Task,
) -> Result<Vec<NodeIDX>> {
    let Some(seed) = seed_candidates(ag, selection, opts, task)? else {
        return Ok(Vec::new());
    };
    let Seed {
        mut candidates,
        properties_applied,
    } = seed;

    if !properties_applied {
        let Some(indices) = bind_properties(ag, selection) else {
            return Ok(Vec::new());
        };
        candidates.retain(|&node_idx| indices.matches(node_idx));
    }

    if opts.reachable_only {
        candidates.retain(|&node_idx| !ag.is_node_unreachable(node_idx));
    }

    let candidates = retain_by_name(ag, candidates, selection)?;
    let candidates = retain_by_edges(
        ag,
        candidates,
        GraphStructure::Reverse,
        selection.incoming_edges(),
    );
    let mut candidates = retain_by_edges(
        ag,
        candidates,
        GraphStructure::Forward,
        selection.outgoing_edges(),
    );

    if let Some(limit) = opts.limit {
        candidates.truncate(limit);
    }

    Ok(candidates)
}

/// Compile a name pattern without running it, so the UI can tell the user their
/// regex is broken while they're still typing it.
///
/// `Fuzzy` and `Exact` have nothing to compile, so they always validate.
pub fn validate_name_match(name_match: &NameMatch) -> Result<()> {
    compile_name_match(name_match).map(|_| ())
}

// ── Seeding ─────────────────────────────────────────────────────

struct Seed {
    candidates: Vec<NodeIDX>,
    /// The seed already enforced the property conditions, so the retain pass
    /// would be redundant work over the same predicate.
    properties_applied: bool,
}

/// The starting candidate set, cheapest generator first.
///
/// `None` means the selection is unsatisfiable — a requested property name is
/// absent from the graph entirely, so no node can meet every condition. That is
/// distinct from an empty `Vec`, which is "nothing happened to match".
fn seed_candidates(
    ag: &ArrayGraph,
    selection: &NodeSelection,
    opts: &SelectOptions,
    task: &ll::Task,
) -> Result<Option<Seed>> {
    match selection.name_condition() {
        Some(name_match) if name_match.mode == NameMatchMode::Exact => {
            let candidates = ag
                .data
                .node_names_ordered
                .name_to_idx_log(&name_match.pattern)
                .into_iter()
                .collect();
            Ok(Some(Seed {
                candidates,
                properties_applied: false,
            }))
        }
        Some(name_match) if name_match.mode == NameMatchMode::Fuzzy => {
            let cap = opts.limit.unwrap_or(DEFAULT_FUZZY_CAP);
            let matches = ag.search_name_fuzzy(&name_match.pattern, cap, task)?;
            Ok(Some(Seed {
                candidates: matches.into_iter().map(|(_, idx)| idx).collect(),
                properties_applied: false,
            }))
        }
        _ => Ok(
            seed_from_properties(ag, selection, opts).map(|candidates| Seed {
                candidates,
                properties_applied: true,
            }),
        ),
    }
}

/// Nodes matching every property condition, ascending.
///
/// Falls back to the whole graph when no properties are asked for, honouring
/// `reachable_only` so the caller doesn't materialise nodes it will drop.
fn seed_from_properties(
    ag: &ArrayGraph,
    selection: &NodeSelection,
    opts: &SelectOptions,
) -> Option<Vec<NodeIDX>> {
    let indices = bind_properties(ag, selection)?;

    if indices.is_empty() {
        return Some(if opts.reachable_only {
            ag.all_reachable_node_idxs()
        } else {
            ag.data.node_names_ordered.node_idx_iter().collect()
        });
    }

    Some(indices.intersect())
}

/// Bind the selection's property conditions to the graph's inverted indices.
///
/// `None` when a requested property name is absent from the graph: that
/// condition can never hold, so the whole selection is unsatisfiable.
fn bind_properties<'a>(
    ag: &'a ArrayGraph,
    selection: &'a NodeSelection,
) -> Option<PropertyIndices<'a>> {
    let conditions = selection
        .properties
        .iter()
        .map(|(name, value_match)| (name.as_str(), value_match.value.as_deref()));
    PropertyIndices::bind(ag, conditions)
}

// ── Retaining ───────────────────────────────────────────────────

/// Drop candidates whose name doesn't match, compiling the pattern once.
///
/// A no-op for `Exact` and `Fuzzy`, which already seeded from the name list.
fn retain_by_name(
    ag: &ArrayGraph,
    mut candidates: Vec<NodeIDX>,
    selection: &NodeSelection,
) -> Result<Vec<NodeIDX>> {
    let Some(name_match) = selection.name_condition() else {
        return Ok(candidates);
    };
    let Some(regex) = compile_name_match(name_match)? else {
        return Ok(candidates);
    };

    candidates.retain(|&node_idx| regex.is_match(ag.idx_to_name(node_idx)));
    Ok(candidates)
}

/// The regex for the predicate modes; `None` for the modes that seed instead.
///
/// Substring escapes the pattern and folds case, which beats
/// `name.to_lowercase().contains(..)` — that allocates a `String` per node, and
/// this pass runs over every candidate.
fn compile_name_match(name_match: &NameMatch) -> Result<Option<Regex>> {
    let pattern = match name_match.mode {
        NameMatchMode::Substring => regex::escape(&name_match.pattern),
        NameMatchMode::Regex => name_match.pattern.clone(),
        NameMatchMode::Fuzzy | NameMatchMode::Exact => return Ok(None),
    };

    RegexBuilder::new(&pattern)
        .case_insensitive(name_match.mode == NameMatchMode::Substring)
        .build()
        .map(Some)
        .context("failed to compile the node name pattern")
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
    use crate::types::array_graph::filter_candidates::filter_candidates;
    use crate::types::array_graph::node_selection::PropertyValueMatch;

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
    impl NodeSelection {
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

        fn substr(self, pattern: &str) -> Self {
            self.name_match(pattern, NameMatchMode::Substring)
        }

        fn re(self, pattern: &str) -> Self {
            self.name_match(pattern, NameMatchMode::Regex)
        }

        fn fuzzy(self, pattern: &str) -> Self {
            self.name_match(pattern, NameMatchMode::Fuzzy)
        }

        fn exact(self, pattern: &str) -> Self {
            self.name_match(pattern, NameMatchMode::Exact)
        }

        fn name_match(mut self, pattern: &str, mode: NameMatchMode) -> Self {
            self.name = Some(NameMatch {
                pattern: pattern.to_string(),
                mode,
            });
            self
        }
    }

    fn sel() -> NodeSelection {
        NodeSelection::default()
    }

    fn all_cases() -> Vec<(&'static str, NodeSelection)> {
        vec![
            ("no conditions", sel()),
            (
                "budget_type=ROUTE",
                sel().prop("budget_type", Some("ROUTE")),
            ),
            (
                "budget_type=ROUTE + team=ads",
                sel()
                    .prop("budget_type", Some("ROUTE"))
                    .prop("team", Some("ads")),
            ),
            ("has budget_type", sel().prop("budget_type", None)),
            ("team=core", sel().prop("team", Some("core"))),
            ("incoming tag lazy", sel().in_tag("lazy")),
            ("incoming tag eager", sel().in_tag("eager")),
            ("incoming dynamic rc:gk", sel().in_dyn("rc:gk")),
            (
                "incoming tag lazy + dynamic rc:gk",
                sel().in_tag("lazy").in_dyn("rc:gk"),
            ),
            (
                "incoming tag lazy + eager",
                sel().in_tag("lazy").in_tag("eager"),
            ),
            (
                "budget_type=ROUTE + incoming tag lazy",
                sel().prop("budget_type", Some("ROUTE")).in_tag("lazy"),
            ),
            (
                "budget_type=PAGE + incoming tag lazy",
                sel().prop("budget_type", Some("PAGE")).in_tag("lazy"),
            ),
            ("outgoing tag lazy", sel().out_tag("lazy")),
            ("outgoing tag eager", sel().out_tag("eager")),
            ("outgoing dynamic rc:gk", sel().out_dyn("rc:gk")),
            (
                "outgoing tag lazy + eager",
                sel().out_tag("lazy").out_tag("eager"),
            ),
            (
                "outgoing tag lazy + dynamic rc:gk",
                sel().out_tag("lazy").out_dyn("rc:gk"),
            ),
            (
                "budget_type=ROUTE + outgoing tag lazy",
                sel().prop("budget_type", Some("ROUTE")).out_tag("lazy"),
            ),
            (
                "incoming tag lazy + outgoing tag lazy",
                sel().in_tag("lazy").out_tag("lazy"),
            ),
            (
                "incoming tag lazy + outgoing tag eager",
                sel().in_tag("lazy").out_tag("eager"),
            ),
            (
                "incoming dynamic rc:gk + outgoing dynamic rc:gk",
                sel().in_dyn("rc:gk").out_dyn("rc:gk"),
            ),
            (
                "incoming tag eager + outgoing tag lazy",
                sel().in_tag("eager").out_tag("lazy"),
            ),
            ("unknown property name", sel().prop("nope", Some("x"))),
            (
                "known name, unknown value",
                sel().prop("budget_type", Some("NOPE")),
            ),
            ("unknown tag", sel().in_tag("nope")),
            ("unknown dynamic type", sel().in_dyn("nope")),
            (
                "known property + unknown tag",
                sel().prop("budget_type", Some("ROUTE")).in_tag("nope"),
            ),
            ("unknown outgoing tag", sel().out_tag("nope")),
            ("unknown outgoing dynamic type", sel().out_dyn("nope")),
            // Name: substring folds case and treats the pattern as literal
            // text; regex is case-sensitive and unanchored unless told
            // otherwise. The `a.` pair is the same string read both ways.
            ("substring eta", sel().substr("eta")),
            ("substring ETA (folds case)", sel().substr("ETA")),
            ("substring a. (escaped)", sel().substr("a.")),
            ("substring blank (no condition)", sel().substr("   ")),
            ("substring nope", sel().substr("nope")),
            ("regex a. (metachar)", sel().re("a.")),
            ("regex ^a (anchored)", sel().re("^a")),
            ("regex a$ (anchored)", sel().re("a$")),
            ("regex ^(alpha|zeta)$", sel().re("^(alpha|zeta)$")),
            ("regex ALPHA (case-sensitive)", sel().re("ALPHA")),
            ("regex (?i)ALPHA", sel().re("(?i)ALPHA")),
            (
                "substring et + budget_type=PAGE",
                sel().substr("et").prop("budget_type", Some("PAGE")),
            ),
            (
                "regex ^.e + incoming tag lazy",
                sel().re("^.e").in_tag("lazy"),
            ),
            // Exact and fuzzy seed from the name list instead of scanning it,
            // so they need their own coverage of the AND with other conditions.
            ("exact alpha", sel().exact("alpha")),
            ("exact ALPHA (case-sensitive)", sel().exact("ALPHA")),
            ("exact al (not a prefix match)", sel().exact("al")),
            ("exact nope", sel().exact("nope")),
            (
                "exact alpha + team=ads",
                sel().exact("alpha").prop("team", Some("ads")),
            ),
            (
                "exact alpha + team=core (excluded)",
                sel().exact("alpha").prop("team", Some("core")),
            ),
            ("fuzzy eta", sel().fuzzy("eta")),
            ("fuzzy ETA (folds case)", sel().fuzzy("ETA")),
            ("fuzzy ata (subsequence)", sel().fuzzy("ata")),
            ("fuzzy nope", sel().fuzzy("nope")),
            (
                "fuzzy a + budget_type=PAGE",
                sel().fuzzy("a").prop("budget_type", Some("PAGE")),
            ),
            (
                "fuzzy a + incoming tag eager",
                sel().fuzzy("a").in_tag("eager"),
            ),
        ]
    }

    /// The options the tree table uses: every match, reachable only.
    fn tree_table_opts() -> SelectOptions {
        SelectOptions {
            limit: None,
            reachable_only: true,
        }
    }

    fn render_cases(ag: &ArrayGraph, cases: &[(&str, NodeSelection)]) -> String {
        render_cases_with(ag, cases, tree_table_opts())
    }

    fn render_cases_with(
        ag: &ArrayGraph,
        cases: &[(&str, NodeSelection)],
        opts: SelectOptions,
    ) -> String {
        let task = ll::Task::create_new("test");
        let rows: Vec<(&str, String)> = cases
            .iter()
            .map(|(label, selection)| {
                let names = match select_nodes(ag, selection, &opts, &task) {
                    Ok(matched) if matched.is_empty() => "(none)".to_string(),
                    Ok(matched) => ag.idxs_to_names(&matched).join(", "),
                    Err(e) => format!("ERROR: {e}"),
                };
                (*label, names)
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
substring eta                                    |  beta, zeta
substring ETA (folds case)                       |  beta, zeta
substring a. (escaped)                           |  (none)
substring blank (no condition)                   |  alpha, beta, delta, epsilon, gamma, root, zeta
substring nope                                   |  (none)
regex a. (metachar)                              |  alpha, gamma
regex ^a (anchored)                              |  alpha
regex a$ (anchored)                              |  alpha, beta, delta, gamma, zeta
regex ^(alpha|zeta)$                             |  alpha, zeta
regex ALPHA (case-sensitive)                     |  (none)
regex (?i)ALPHA                                  |  alpha
substring et + budget_type=PAGE                  |  zeta
regex ^.e + incoming tag lazy                    |  beta, delta
exact alpha                                      |  alpha
exact ALPHA (case-sensitive)                     |  (none)
exact al (not a prefix match)                    |  (none)
exact nope                                       |  (none)
exact alpha + team=ads                           |  alpha
exact alpha + team=core (excluded)               |  (none)
fuzzy eta                                        |  beta, zeta, delta
fuzzy ETA (folds case)                           |  beta, zeta, delta
fuzzy ata (subsequence)                          |  (none)
fuzzy nope                                       |  (none)
fuzzy a + budget_type=PAGE                       |  zeta, gamma
fuzzy a + incoming tag eager                     |  (none)
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
                sel().prop("budget_type", Some("ROUTE")),
            ),
            ("incoming tag lazy", sel().in_tag("lazy")),
            ("incoming dynamic rc:gk", sel().in_dyn("rc:gk")),
            ("outgoing dynamic rc:gk", sel().out_dyn("rc:gk")),
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
            ("incoming tag eager", sel().in_tag("eager")),
            ("outgoing tag eager", sel().out_tag("eager")),
            ("outgoing tag lazy", sel().out_tag("lazy")),
            ("team=core", sel().prop("team", Some("core"))),
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

    /// A broken regex has to surface as an error the UI can show, not as a
    /// silent empty result that reads like "nothing matched".
    #[test]
    fn test_invalid_regex_is_an_error() -> Result<()> {
        let ag = test_graph()?;
        let task = ll::Task::create_new("test");

        let cases = [
            ("unclosed class", sel().re("foo[")),
            ("dangling repeat", sel().re("*bar")),
            ("unmatched paren", sel().re("(a")),
            // Rust's regex crate rejects backreferences, so a pattern that is
            // valid in JS can still fail here — the reason validation is a
            // WASM round trip rather than `new RegExp` on the frontend.
            ("backreference", sel().re(r"(a)\1")),
        ];

        let errors: Vec<String> = cases
            .iter()
            .map(|(label, s)| {
                let outcome = match select_nodes(&ag, s, &tree_table_opts(), &task) {
                    Ok(_) => "accepted".to_string(),
                    Err(e) => format!("rejected: {e}"),
                };
                format!("{label:<16}|  {outcome}")
            })
            .collect();

        snapshot!(
            errors.join("\n"),
            "
unclosed class  |  rejected: failed to compile the node name pattern
dangling repeat |  rejected: failed to compile the node name pattern
unmatched paren |  rejected: failed to compile the node name pattern
backreference   |  rejected: failed to compile the node name pattern
"
        );

        // The same patterns are what `validate_name_match` guards against, so
        // the UI never has to run the filter to find out.
        assert!(
            validate_name_match(&NameMatch {
                pattern: "foo[".to_string(),
                mode: NameMatchMode::Regex,
            })
            .is_err(),
            "an unclosed character class should fail validation"
        );
        assert!(
            validate_name_match(&NameMatch {
                pattern: "foo[".to_string(),
                mode: NameMatchMode::Substring,
            })
            .is_ok(),
            "the same text is a literal in substring mode, so it must validate"
        );
        for mode in [NameMatchMode::Fuzzy, NameMatchMode::Exact] {
            assert!(
                validate_name_match(&NameMatch {
                    pattern: "foo[".to_string(),
                    mode,
                })
                .is_ok(),
                "{mode:?} never compiles a pattern, so nothing can fail validation"
            );
        }

        Ok(())
    }

    /// `reachable_only` is what separates the tree table from a raw name search:
    /// the search still finds a node the traversal config pruned.
    #[test]
    fn test_reachable_only_is_opt_in() -> Result<()> {
        let mut ag = test_graph()?;
        let tvc: TraversalConfig =
            serde_json::from_str(r#"{"force_nodes": {"delta": {"include": false}}}"#)?;
        ag.apply_traversal_config_and_entry_points(tvc)?;

        let cases = [
            (
                "budget_type=ROUTE",
                sel().prop("budget_type", Some("ROUTE")),
            ),
            ("exact delta", sel().exact("delta")),
            ("fuzzy dta", sel().fuzzy("dta")),
        ];

        let unreachable_included = SelectOptions {
            limit: None,
            reachable_only: false,
        };

        snapshot!(
            format!(
                "reachable_only: true\n{}\n\nreachable_only: false\n{}",
                render_cases(&ag, &cases),
                render_cases_with(&ag, &cases, unreachable_included),
            ),
            "
reachable_only: true
budget_type=ROUTE  |  alpha, beta
exact delta        |  (none)
fuzzy dta          |  (none)

reachable_only: false
budget_type=ROUTE  |  alpha, beta, delta
exact delta        |  delta
fuzzy dta          |  delta
"
        );
        Ok(())
    }

    /// `Fuzzy` is top-K, so the cap decides how much of the match set is even
    /// enumerated. This is why callers that need a true total (`ExploreGraph`)
    /// reject the mode outright rather than reporting a capped count.
    #[test]
    fn test_fuzzy_respects_the_cap() -> Result<()> {
        let ag = test_graph()?;

        // "a" is a subsequence of six of the seven nodes, so the cap bites well
        // before the match set is exhausted.
        let capped = |limit: usize| SelectOptions {
            limit: Some(limit),
            reachable_only: true,
        };
        let cases = [("fuzzy a", sel().fuzzy("a"))];

        snapshot!(
            format!(
                "limit 2\n{}\n\nlimit 100\n{}",
                render_cases_with(&ag, &cases, capped(2)),
                render_cases_with(&ag, &cases, capped(100)),
            ),
            "
limit 2
fuzzy a  |  beta, zeta

limit 100
fuzzy a  |  beta, zeta, alpha, delta, gamma
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
