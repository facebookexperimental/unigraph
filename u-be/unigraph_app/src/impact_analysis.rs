// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Impact analysis: brute-force simulation of node removal.
//!
//! # Motivation
//!
//! When exploring a dependency graph to find size reduction opportunities,
//! the dominator tree is the go-to tool: it shows subtrees "owned" by a
//! single edge — cut that edge and the entire subtree disappears. But the
//! dominator tree has a blind spot: when a node has 2+ parents, its
//! dominator jumps all the way to the root, and we lose all visibility
//! into what's behind it. In practice, many nodes are "almost dominated" —
//! connected to the main graph by just 2-3 edges, with a huge subgraph
//! behind them that could be removed if those few edges were cut.
//!
//! # Approach
//!
//! For each node in the graph, we simulate what happens if we remove it:
//! force-exclude the node, reapply the traversal config, and measure how
//! total graph metrics change. This is O(N × (V + E)) — expensive, but
//! correct, parallelizable, and simple to reason about.
//!
//! The `max_parents` filter makes it practical: nodes with 100 parents are
//! never going to be easy wins, so we skip them and focus on the 2-5
//! parent range where a small number of edge cuts can have outsized impact.
//!
//! # Output metrics
//!
//! Results are per-node `Vec<f32>` arrays in the same shape as
//! `ArrayGraph.metrics`, so they can be injected directly into the graph
//! and immediately show up in the tree table for sorting/filtering.
//!
//! - **`impact_count`**: how many nodes become unreachable
//! - **`impact_{metric}`**: how much a flat metric drops (e.g. `impact_size`)
//! - **`impact_{metric}_{tier}`**: how much a tiered metric drops
//! - **`impact_leverage_count`**: impact_count / parent_count
//! - **`impact_leverage_{metric}`**: impact / parent_count
//!
//! The leverage variants are the key signal: high leverage with low parent
//! count = "cut these 2 edges and 500 nodes disappear."
//!
//! # Threading model
//!
//! Candidates are partitioned into chunks (one per CPU core). Each thread
//! gets its own `ArrayGraph` (constructed from a cloned `ArrayGraphSerializable`,
//! where `Arc<ArrayGraphNodes>` is shared and cheap). Each thread reuses its
//! `ArrayGraph` across the entire batch — mutating traversal config per
//! candidate, snapshotting metrics, then moving to the next.
//!
//! # Caveats
//!
//! - **Entry points always show zero impact.** Force-excluding a root node
//!   has no effect because it has no incoming edges to cut. This is correct —
//!   you can't "remove" a root by cutting dependencies.
//!
//! - **`to_serializable()` clones the full graph once.** For very large graphs
//!   this is a significant allocation, but it only happens once (not per node).
//!
//! - **`apply_traversal_config` is the per-node bottleneck.** Each iteration
//!   rebuilds reverse edges and clears OnceLock caches. The actual DFS is fast;
//!   the setup overhead dominates.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use anyhow::Result;
use rayon::prelude::*;
use unigraph_core::ArrayGraph;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::CombinedMetricsForNodes;
use unigraph_core::Decision;
use unigraph_core::TraversalConfig;
use unigraph_core::graph_settings::MetricFormat;
use unigraph_core::graph_settings::MetricViewSettings;
use unigraph_core::types::NodeIDX;

// ── Public API ──────────────────────────────────────────────────────────

/// Owns the graph and runs impact analysis on it, patching
/// the graph with computed impact metrics and their settings.
pub struct ImpactAnalysis {
    pub ag: ArrayGraph,
    /// Only analyze nodes with at most this many parents.
    /// `None` = analyze all reachable nodes.
    pub max_parents: Option<usize>,
}

impl ImpactAnalysis {
    /// Run impact analysis: simulate excluding each candidate node,
    /// measure the delta, and patch `self.ag` with the results.
    #[ll::task(sync)]
    pub fn run(mut self, task: &ll::Task) -> Result<ArrayGraph> {
        let baseline = compute_baseline(&self.ag, &task)?;
        let candidates = collect_candidates(&self.ag, self.max_parents);
        let base_tc = extract_base_traversal_config(&self.ag);
        let ags = self.ag.to_serializable();

        task.data("candidates", candidates.len());
        task.data("nodes_total", self.ag.nodes_len());

        let per_node_results = run_parallel_simulations(&ags, &base_tc, &candidates, &task)?;
        let impact_metrics = build_impact_metrics(&self.ag, &baseline, &per_node_results);

        patch_graph(&mut self.ag, impact_metrics);

        Ok(self.ag)
    }
}

// ── Internals ───────────────────────────────────────────────────────────

fn compute_baseline(ag: &ArrayGraph, task: &ll::Task) -> Result<CombinedMetricsForNodes> {
    let ags = ag.to_serializable();
    let mut baseline_ag: ArrayGraph = ags.into_array_graph(task)?;
    if let Some(tc) = ag.runtime.state.traversal_config.clone() {
        baseline_ag.apply_traversal_config(tc)?;
    }
    baseline_ag.get_combined_metrics_for_entry_points(None)
}

fn collect_candidates(ag: &ArrayGraph, max_parents: Option<usize>) -> Vec<NodeIDX> {
    ag.node_idx_iter_reachable()
        .filter(|&idx| match max_parents {
            Some(max) => ag.parents_len_configured(idx) <= max,
            None => true,
        })
        .collect()
}

fn extract_base_traversal_config(ag: &ArrayGraph) -> TraversalConfig {
    ag.runtime
        .state
        .traversal_config
        .clone()
        .unwrap_or_default()
}

const PROGRESS_EVERY_N: usize = 50;

fn run_parallel_simulations(
    ags: &ArrayGraphSerializable,
    base_tc: &TraversalConfig,
    candidates: &[NodeIDX],
    task: &ll::Task,
) -> Result<Vec<(NodeIDX, CombinedMetricsForNodes)>> {
    let num_threads = rayon::current_num_threads().max(1);
    let chunks = partition_into_chunks(candidates, num_threads);
    let total = candidates.len();
    let done = AtomicUsize::new(0);

    task.progress(0, total as i64);

    let thread_results: Vec<Vec<(NodeIDX, CombinedMetricsForNodes)>> = chunks
        .into_par_iter()
        .map(|chunk| simulate_chunk(ags, base_tc, &chunk, &done, total, task))
        .collect::<Result<Vec<_>>>()?;

    task.progress(total as i64, total as i64);

    Ok(thread_results.into_iter().flatten().collect())
}

fn simulate_chunk(
    ags: &ArrayGraphSerializable,
    base_tc: &TraversalConfig,
    chunk: &[NodeIDX],
    done: &AtomicUsize,
    total: usize,
    task: &ll::Task,
) -> Result<Vec<(NodeIDX, CombinedMetricsForNodes)>> {
    let mut thread_ag: ArrayGraph = ags.clone().into_array_graph(task)?;
    let mut results = Vec::with_capacity(chunk.len());

    for &node_idx in chunk {
        let node_name = thread_ag.idx_to_name(node_idx).to_string();
        let tc = make_exclude_config(base_tc, &node_name);
        thread_ag.apply_traversal_config(tc)?;

        let metrics = thread_ag.get_combined_metrics_for_entry_points(None)?;
        results.push((node_idx, metrics));

        let prev = done.fetch_add(1, Ordering::Relaxed);
        if (prev + 1).is_multiple_of(PROGRESS_EVERY_N) {
            task.progress((prev + 1) as i64, total as i64);
        }
    }

    Ok(results)
}

fn make_exclude_config(base_tc: &TraversalConfig, node_name: &str) -> TraversalConfig {
    let mut tc = base_tc.clone();
    let force_nodes = tc.force_nodes.get_or_insert_with(BTreeMap::new);
    force_nodes.insert(node_name.to_string(), Decision::exclude());
    tc
}

// ── Impact metric descriptors ───────────────────────────────────────────

/// Describes a single impact metric to compute.
/// Carries the source metric reference structurally so we never
/// have to reverse-engineer it from the output key string.
enum ImpactMetric {
    /// How many nodes become unreachable.
    Count,
    /// How much a flat metric drops. Source: `metric_name`.
    Metric { metric_name: String },
    /// How much a tiered metric drops. Source: `metric_name`, tier: `tier_name`.
    TieredMetric {
        metric_name: String,
        tier_name: String,
    },
    /// Any of the above divided by parent count.
    Leverage(Box<ImpactMetric>),
}

impl ImpactMetric {
    /// The key used in `ArrayGraph.metrics` for this impact metric.
    fn key(&self) -> String {
        match self {
            ImpactMetric::Count => "impact_count".into(),
            ImpactMetric::Metric { metric_name } => format!("impact_{metric_name}"),
            ImpactMetric::TieredMetric {
                metric_name,
                tier_name,
            } => format!("impact_{metric_name}_{tier_name}"),
            ImpactMetric::Leverage(inner) => inner.key().replace("impact_", "impact_leverage_"),
        }
    }

    /// The source metric name to inherit format from, if any.
    fn source_metric_name(&self) -> Option<&str> {
        match self {
            ImpactMetric::Count => None,
            ImpactMetric::Metric { metric_name } => Some(metric_name),
            ImpactMetric::TieredMetric { metric_name, .. } => Some(metric_name),
            ImpactMetric::Leverage(inner) => inner.source_metric_name(),
        }
    }

    fn is_leverage(&self) -> bool {
        matches!(self, ImpactMetric::Leverage(_))
    }

    fn description(&self) -> String {
        match self {
            ImpactMetric::Count => concat!(
                "How many nodes become unreachable if this node is removed. ",
                "Higher values mean this node is a critical gateway \u{2014} ",
                "a large subgraph depends on it."
            )
            .into(),

            ImpactMetric::Metric { metric_name } => format!(
                "How much the total '{metric_name}' metric drops if this node \
                 is removed from the graph. Measures the actual cost \
                 attributable to this node's presence."
            ),

            ImpactMetric::TieredMetric {
                metric_name,
                tier_name,
            } => format!(
                "How much '{metric_name}' at tier '{tier_name}' drops \
                 if this node is removed."
            ),

            ImpactMetric::Leverage(inner) => match inner.as_ref() {
                ImpactMetric::Count => concat!(
                    "Impact count divided by parent count. ",
                    "High leverage means removing this node has outsized effect ",
                    "relative to how many edges you'd need to cut. ",
                    "Sort by this to find easy wins."
                )
                .into(),

                _ => {
                    let source = inner.source_metric_name().unwrap_or("metric");
                    format!(
                        "Impact on '{source}' divided by parent count. \
                         High leverage = large metric reduction per edge cut. \
                         The higher this value with fewer parents, the easier the win."
                    )
                }
            },
        }
    }

    fn format(&self, source_format: Option<&MetricFormat>) -> Option<MetricFormat> {
        if self.is_leverage() || matches!(self, ImpactMetric::Count) {
            Some(MetricFormat::NumberWithVariablePrecision {
                min_precision: Some(0),
                max_precision: Some(0),
                use_delimiter: Some(true),
            })
        } else {
            source_format.cloned()
        }
    }
}

// ── Build impact metrics ────────────────────────────────────────────────

fn build_impact_metrics(
    ag: &ArrayGraph,
    baseline: &CombinedMetricsForNodes,
    results: &[(NodeIDX, CombinedMetricsForNodes)],
) -> Vec<(ImpactMetric, Vec<f32>)> {
    let n = ag.nodes_len();
    let mut output = Vec::new();

    // Count
    output.push(compute_count(baseline, results, n));

    // Flat metrics
    for (metric_name, &baseline_v) in &baseline.metrics {
        output.push(compute_flat_metric(metric_name, baseline_v, results, n));
    }

    // Tiered metrics
    for (metric_name, tier_map) in &baseline.tiered_metrics {
        for (tier_name, &baseline_v) in tier_map {
            output.push(compute_tiered_metric(
                metric_name,
                tier_name,
                baseline_v,
                results,
                n,
            ));
        }
    }

    // Leverage variants for everything computed so far
    let leverage: Vec<(ImpactMetric, Vec<f32>)> = output
        .iter()
        .map(|entry| compute_leverage(entry, ag, results, n))
        .collect();
    output.extend(leverage);

    output
}

fn compute_count(
    baseline: &CombinedMetricsForNodes,
    results: &[(NodeIDX, CombinedMetricsForNodes)],
    n: usize,
) -> (ImpactMetric, Vec<f32>) {
    let mut values = vec![0.0; n];
    for &(idx, ref r) in results {
        values[idx] = (baseline.node_count as f32) - (r.node_count as f32);
    }
    (ImpactMetric::Count, values)
}

fn compute_flat_metric(
    metric_name: &str,
    baseline_v: f32,
    results: &[(NodeIDX, CombinedMetricsForNodes)],
    n: usize,
) -> (ImpactMetric, Vec<f32>) {
    let mut values = vec![0.0; n];
    for &(idx, ref r) in results {
        let v = r.metrics.get(metric_name).copied().unwrap_or(0.0);
        values[idx] = baseline_v - v;
    }
    let metric = ImpactMetric::Metric {
        metric_name: metric_name.to_string(),
    };
    (metric, values)
}

fn compute_tiered_metric(
    metric_name: &str,
    tier_name: &str,
    baseline_v: f32,
    results: &[(NodeIDX, CombinedMetricsForNodes)],
    n: usize,
) -> (ImpactMetric, Vec<f32>) {
    let mut values = vec![0.0; n];
    for &(idx, ref r) in results {
        let v = r
            .tiered_metrics
            .get(metric_name)
            .and_then(|t| t.get(tier_name))
            .copied()
            .unwrap_or(0.0);
        values[idx] = baseline_v - v;
    }
    let metric = ImpactMetric::TieredMetric {
        metric_name: metric_name.to_string(),
        tier_name: tier_name.to_string(),
    };
    (metric, values)
}

fn compute_leverage(
    source: &(ImpactMetric, Vec<f32>),
    ag: &ArrayGraph,
    results: &[(NodeIDX, CombinedMetricsForNodes)],
    n: usize,
) -> (ImpactMetric, Vec<f32>) {
    let (source_metric, source_vals) = source;
    let mut leverage = vec![0.0; n];
    for &(idx, _) in results {
        let parents = ag.parents_len_configured(idx).max(1) as f32;
        leverage[idx] = source_vals[idx] / parents;
    }
    let metric = ImpactMetric::Leverage(Box::new(match source_metric {
        ImpactMetric::Count => ImpactMetric::Count,
        ImpactMetric::Metric { metric_name } => ImpactMetric::Metric {
            metric_name: metric_name.clone(),
        },
        ImpactMetric::TieredMetric {
            metric_name,
            tier_name,
        } => ImpactMetric::TieredMetric {
            metric_name: metric_name.clone(),
            tier_name: tier_name.clone(),
        },
        ImpactMetric::Leverage(_) => unreachable!("leverage of leverage"),
    }));
    (metric, leverage)
}

// ── Patch graph ─────────────────────────────────────────────────────────

fn patch_graph(ag: &mut ArrayGraph, impact_metrics: Vec<(ImpactMetric, Vec<f32>)>) {
    // Snapshot existing metric settings before mutating
    let existing_settings = ag
        .data
        .graph_settings
        .as_ref()
        .and_then(|gs| gs.ui_settings.as_ref())
        .and_then(|ui| ui.columns.as_ref())
        .and_then(|c| c.metric_settings.as_ref())
        .cloned()
        .unwrap_or_default();

    let settings = ag
        .data
        .graph_settings
        .get_or_insert_with(Default::default)
        .ui_settings
        .get_or_insert_with(Default::default)
        .columns
        .get_or_insert_with(Default::default)
        .metric_settings
        .get_or_insert_with(BTreeMap::new);

    for (metric, values) in impact_metrics {
        let key = metric.key();

        let source_format = metric
            .source_metric_name()
            .and_then(|src| existing_settings.get(src))
            .and_then(|s| s.format.as_ref());

        settings.insert(
            key.clone(),
            MetricViewSettings {
                description: Some(metric.description()),
                format: metric.format(source_format),
                visibility: Some(unigraph_core::graph_settings::MetricViewVisibility::Hidden {}),
            },
        );

        ag.data.node_metadata.metrics.insert(key, values);
    }
}

fn partition_into_chunks(items: &[NodeIDX], num_chunks: usize) -> Vec<Vec<NodeIDX>> {
    if items.is_empty() || num_chunks == 0 {
        return vec![];
    }
    let chunk_size = items.len().div_ceil(num_chunks);
    items.chunks(chunk_size).map(|c| c.to_vec()).collect()
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::snapshot;
    use unigraph_core::types::MapGraph;

    use super::*;

    /// Diamond: A -> B -> C, A -> D -> C
    /// C has 2 parents (B and D).
    const DIAMOND_GRAPH: &str = r#"{
        "nodes": {
            "A": { "edges_directed": ["B", "D"], "metrics": { "size": 10.0 } },
            "B": { "edges_directed": ["C"], "metrics": { "size": 20.0 } },
            "C": { "metrics": { "size": 30.0 } },
            "D": { "edges_directed": ["C"], "metrics": { "size": 40.0 } }
        }
    }"#;

    /// Linear: A -> B -> C -> D
    const CHAIN_GRAPH: &str = r#"{
        "nodes": {
            "A": { "edges_directed": ["B"], "metrics": { "size": 1.0 } },
            "B": { "edges_directed": ["C"], "metrics": { "size": 2.0 } },
            "C": { "edges_directed": ["D"], "metrics": { "size": 3.0 } },
            "D": { "metrics": { "size": 4.0 } }
        }
    }"#;

    fn make_graph(json: &str) -> Result<ArrayGraph> {
        MapGraph::from_json(json)?.to_array_graph(&ll::Task::create_new("test"))
    }

    fn run(ag: ArrayGraph, max_parents: Option<usize>) -> Result<ArrayGraph> {
        let rt = tokio::runtime::Runtime::new()?;
        let task = rt.block_on(async { ll::Task::create_new("test") });
        let analysis = ImpactAnalysis { ag, max_parents };
        analysis.run(&task)
    }

    fn format_impact_table(ag: &ArrayGraph) -> String {
        let mut metric_names: Vec<&String> = ag
            .data
            .node_metadata
            .metrics
            .keys()
            .filter(|k| k.starts_with("impact_"))
            .collect();
        metric_names.sort();

        let mut out = format!("{:<6}", "node");
        for name in &metric_names {
            out.push_str(&format!("  {:>20}", name));
        }
        out.push('\n');

        for node_idx in ag.node_idx_iter() {
            if ag.is_node_unreachable(node_idx) {
                continue;
            }
            let name = ag.idx_to_name(node_idx);
            out.push_str(&format!("{:<6}", name));
            for metric_name in &metric_names {
                let v = ag.data.node_metadata.metrics[*metric_name][node_idx];
                if v == 0.0 {
                    out.push_str(&format!("  {:>20}", "-"));
                } else {
                    out.push_str(&format!("  {:>20}", v));
                }
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn test_diamond() -> Result<()> {
        let ag = run(make_graph(DIAMOND_GRAPH)?, None)?;
        snapshot!(
            format_impact_table(&ag),
            "
node            impact_count  impact_leverage_count  impact_leverage_size           impact_size
A                          -                     -                     -                     -
B                          1                     1                    20                    20
C                          1                   0.5                    15                    30
D                          1                     1                    40                    40

"
        );
        Ok(())
    }

    #[test]
    fn test_chain() -> Result<()> {
        let ag = run(make_graph(CHAIN_GRAPH)?, None)?;
        snapshot!(
            format_impact_table(&ag),
            "
node            impact_count  impact_leverage_count  impact_leverage_size           impact_size
A                          -                     -                     -                     -
B                          3                     3                     9                     9
C                          2                     2                     7                     7
D                          1                     1                     4                     4

"
        );
        Ok(())
    }

    #[test]
    fn test_max_parents_filter() -> Result<()> {
        let ag = run(make_graph(DIAMOND_GRAPH)?, Some(1))?;
        snapshot!(
            format_impact_table(&ag),
            "
node            impact_count  impact_leverage_count  impact_leverage_size           impact_size
A                          -                     -                     -                     -
B                          1                     1                    20                    20
C                          -                     -                     -                     -
D                          1                     1                    40                    40

"
        );
        Ok(())
    }

    #[test]
    fn test_metric_settings_added() -> Result<()> {
        let ag = run(make_graph(CHAIN_GRAPH)?, None)?;

        let settings = ag
            .data
            .graph_settings
            .as_ref()
            .unwrap()
            .ui_settings
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .metric_settings
            .as_ref()
            .unwrap();

        // All impact metrics should have settings with descriptions
        for key in ag
            .data
            .node_metadata
            .metrics
            .keys()
            .filter(|k| k.starts_with("impact_"))
        {
            let s = settings.get(key).unwrap_or_else(|| {
                panic!("missing metric settings for {key}");
            });
            assert!(s.description.is_some(), "missing description for {key}");
            assert_eq!(
                s.visibility,
                Some(unigraph_core::graph_settings::MetricViewVisibility::Hidden {}),
                "impact metrics should be hidden for {key}"
            );
        }

        Ok(())
    }
}
