// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;

use crate::ArrayGraph;
use crate::NodeIDX;
use crate::types::MetricName;
use crate::types::TierIDX;
use crate::types::TierName;
use crate::types::array_graph::offset_graph::OffsetGraph;

const MISSING: usize = usize::MAX;
#[allow(clippy::upper_case_acronyms)]
type SCCIDX = usize;

/// Conjoint cost of the component is a value that represents its transitive
/// size adjusted for how many other nodes it depends on.
/// It's calculated by summing up the cost of all ConjCost(direct children) and
/// dividing it by the number of parents.
///
/// This way people will be penalized less for things that are popular.
/// E.g. if there is a popular framework that almost every single node
/// uses it would not make sense for it to try to remove that depenedncy, since
/// it will likely still stay in the graph.
#[derive(serde::Serialize, typegen::TypeGen)]
pub struct ConjointCost {
    pub count: Vec<f32>,
    pub metrics: BTreeMap<MetricName, Vec<f32>>,
    pub tiered_metric: BTreeMap<MetricName, BTreeMap<TierName, Vec<f32>>>,
}

impl ConjointCost {
    pub fn build(ag: &ArrayGraph) -> Result<Self> {
        ConjointCostBuilder::new(ag).build()
    }
}

struct ConjointCostBuilder<'a> {
    ag: &'a ArrayGraph,
    scc_conj_counts: Vec<f32>,
    scc_conj_metrics: BTreeMap<MetricName, Vec<f32>>,
    scc_conj_tiered_metrics: BTreeMap<MetricName, Vec<Vec<f32>>>,
    node_idx_to_scc_idx: Vec<SCCIDX>,
    scc: &'a [Vec<NodeIDX>],
}

impl<'a> ConjointCostBuilder<'a> {
    pub fn new(ag: &'a ArrayGraph) -> Self {
        let sccs = ag.sccs();

        let scc_conj_counts = vec![0.0; sccs.len()];
        let scc_conj_metrics: BTreeMap<MetricName, Vec<f32>> = ag
            .metrics
            .keys()
            .map(|metric_name| (metric_name.clone(), vec![0.0; sccs.len()]))
            .collect();
        let scc_conj_tiered_metrics: BTreeMap<MetricName, Vec<Vec<f32>>> = ag
            .metrics
            .keys()
            .map(|metric_name| {
                let tiered: Vec<Vec<f32>> = ag
                    .state
                    .tiers
                    .iter()
                    .map(|_| vec![0.0; sccs.len()])
                    .collect::<_>();
                (metric_name.clone(), tiered)
            })
            .collect();

        let mut node_idx_to_scc_idx = vec![MISSING; ag.nodes_len()];
        for (scc_idx, scc_nodes) in sccs.iter().enumerate() {
            for &node_idx in scc_nodes {
                node_idx_to_scc_idx[node_idx] = scc_idx;
            }
        }

        Self {
            ag,
            scc_conj_counts,
            scc_conj_metrics,
            scc_conj_tiered_metrics,
            node_idx_to_scc_idx,
            scc: sccs,
        }
    }

    pub fn build(mut self) -> Result<ConjointCost> {
        // SCCs are reversed topologically sorted, so we can process them in reverse order.
        // This is a bottom-up traversal of the SCCs.
        // meaning that when we process an SCC, all of its children have already been processed
        // and their values have been calculated.
        for (scc_idx, scc) in self.scc.iter().enumerate() {
            let scc_children = self.get_scc_edges(scc, &self.ag.edges_forward);
            let scc_parents = self.get_scc_edges(scc, &self.ag.derived_state.edges_reverse);

            let uniq_parents_count = scc_parents.len();

            self.add_self_cost(scc_idx, scc)?;

            for &child_scc_idx in &scc_children {
                self.add_child_cost(scc_idx, child_scc_idx)?;
            }

            self.divide_cost_by_parents_count(scc_idx, uniq_parents_count)?;
        }
        self.into_node_idx_conj_cost()
    }

    /// Build the edges (parents or children) between SCCs based on the edges
    /// the individual nodes in the SCC have
    fn get_scc_edges(&self, scc: &[NodeIDX], offset_graph: &OffsetGraph) -> BTreeSet<SCCIDX> {
        // this is a set because we need to deduplicate the edges.
        // Since we're working on multiple nodes within the same SCC,
        // it's possible that multiple nodes point to the same parent or child.
        let mut scc_node_idx_edges: BTreeSet<NodeIDX> = BTreeSet::new();

        for &node_idx in scc {
            scc_node_idx_edges.extend(
                offset_graph
                    .edges_configured(node_idx)
                    .map(|edge| edge.points_to),
            );
        }

        // now we need to map these edges to their SCC indexes, because that's what
        // the algorithm works with.
        let mut scc_scc_idx_edges: BTreeSet<SCCIDX> = BTreeSet::new();
        for parent_node_idx in scc_node_idx_edges {
            let parent_scc_idx = self.node_idx_to_scc_idx[parent_node_idx];
            if parent_scc_idx != MISSING {
                scc_scc_idx_edges.insert(parent_scc_idx);
            }
        }
        scc_scc_idx_edges
    }

    fn add_self_cost(&mut self, scc_idx: SCCIDX, scc: &[NodeIDX]) -> Result<()> {
        let count = scc.len() as f32;
        self.scc_conj_counts[scc_idx] = count;

        for &node_idx in scc {
            for (metric_name, metrics) in &self.ag.metrics {
                let current_v = self.get_scc_value_for_metric(metric_name, scc_idx)?;
                let node_v = metrics[node_idx];

                self.set_scc_value_for_metric(metric_name, scc_idx, current_v + node_v)?;

                if !self.ag.state.tiers.is_empty() {
                    let tier_idx = self.ag.try_node_tier_idx(node_idx)?;
                    let current_v_tiered =
                        self.get_scc_value_for_tiered_metric(metric_name, tier_idx, scc_idx)?;
                    self.set_scc_value_for_tiered_metric(
                        metric_name,
                        tier_idx,
                        scc_idx,
                        current_v_tiered + node_v,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn add_child_cost(&mut self, scc_idx: SCCIDX, child_scc_idx: SCCIDX) -> Result<()> {
        let child_count = self.scc_conj_counts[child_scc_idx];
        self.scc_conj_counts[scc_idx] += child_count;

        for metric_name in self.ag.metrics.keys() {
            let current_v = self.get_scc_value_for_metric(metric_name, scc_idx)?;
            let child_v = self.get_scc_value_for_metric(metric_name, child_scc_idx)?;
            self.set_scc_value_for_metric(metric_name, scc_idx, current_v + child_v)?;

            for (_tier_name, tier_idx) in &self.ag.state.tiers {
                let current_v =
                    self.get_scc_value_for_tiered_metric(metric_name, *tier_idx, scc_idx)?;
                let child_v =
                    self.get_scc_value_for_tiered_metric(metric_name, *tier_idx, child_scc_idx)?;
                self.set_scc_value_for_tiered_metric(
                    metric_name,
                    *tier_idx,
                    scc_idx,
                    current_v + child_v,
                )?;
            }
        }
        Ok(())
    }

    fn get_scc_value_for_metric(&self, metric_name: &String, scc_idx: SCCIDX) -> Result<f32> {
        self.scc_conj_metrics
            .get(metric_name)
            .as_ref()
            .with_context(|| format!("[conj cost] Missing scc metric name: {metric_name}"))
            .map(|metrics| metrics[scc_idx])
    }

    fn set_scc_value_for_metric(
        &mut self,
        metric_name: &String,
        scc_idx: SCCIDX,
        value: f32,
    ) -> Result<()> {
        self.scc_conj_metrics
            .get_mut(metric_name)
            .as_mut()
            .with_context(|| format!("[conj cost] Missing scc metric name: {metric_name}"))?
            [scc_idx] = value;
        Ok(())
    }

    fn get_scc_value_for_tiered_metric(
        &self,
        metric_name: &String,
        tier_idx: TierIDX,
        scc_idx: SCCIDX,
    ) -> Result<f32> {
        self.scc_conj_tiered_metrics
            .get(metric_name)
            .with_context(|| format!("[conj cost] Missing scc tiered metric name: {metric_name}"))?
            .get(tier_idx)
            .with_context(|| {
                format!(
                    "[conj cost] Missing scc tiered metric name: {metric_name} for tier {tier_idx}"
                )
            })
            .map(|m| m[scc_idx])
    }

    fn set_scc_value_for_tiered_metric(
        &mut self,
        metric_name: &String,
        tier_idx: TierIDX,
        scc_idx: SCCIDX,
        value: f32,
    ) -> Result<()> {
        self.scc_conj_tiered_metrics
            .get_mut(metric_name)
            .with_context(|| format!("[conj cost] Missing scc tiered metric name: {metric_name}"))?
            .get_mut(tier_idx)
            .with_context(|| {
                format!(
                    "[conj cost] Missing scc tiered metric name: {metric_name} for tier {tier_idx}"
                )
            })?[scc_idx] = value;
        Ok(())
    }

    fn divide_cost_by_parents_count(&mut self, scc_idx: usize, count: usize) -> Result<()> {
        // Avoid division by zero. If there are no parents then the cost should be
        // the cost of all children combined withoud division
        let count = count.max(1) as f32;

        self.scc_conj_counts[scc_idx] /= count;

        for metric_name in self.ag.metrics.keys() {
            let v = self.get_scc_value_for_metric(metric_name, scc_idx)?;
            self.set_scc_value_for_metric(metric_name, scc_idx, v / count)?;

            for (_tier_name, tier_idx) in &self.ag.state.tiers {
                let v = self.get_scc_value_for_tiered_metric(metric_name, *tier_idx, scc_idx)?;
                self.set_scc_value_for_tiered_metric(metric_name, *tier_idx, scc_idx, v / count)?;
            }
        }
        Ok(())
    }

    fn into_node_idx_conj_cost(self) -> Result<ConjointCost> {
        let mut result = ConjointCost {
            count: vec![0.0; self.ag.nodes_len()],
            metrics: BTreeMap::new(),
            tiered_metric: BTreeMap::new(),
        };

        for node_idx in self.ag.node_idx_iter_reachable() {
            let scc_idx = self.node_idx_to_scc_idx[node_idx];
            result.count[node_idx] = self.scc_conj_counts[scc_idx];
        }

        for metric_name in self.ag.metrics.keys() {
            let mut for_metric = vec![0.0; self.ag.nodes_len()];
            let mut for_tiered_metric = BTreeMap::new();

            for node_idx in self.ag.node_idx_iter_reachable() {
                let scc_idx = self.node_idx_to_scc_idx[node_idx];
                let conj_value = self.get_scc_value_for_metric(metric_name, scc_idx)?;
                for_metric[node_idx] = conj_value;
            }

            for (tier_name, tier_idx) in &self.ag.state.tiers {
                let mut for_tier = vec![0.0; self.ag.nodes_len()];

                for node_idx in self.ag.node_idx_iter_reachable() {
                    let scc_idx = self.node_idx_to_scc_idx[node_idx];
                    let conj_value_tiered =
                        self.get_scc_value_for_tiered_metric(metric_name, *tier_idx, scc_idx)?;
                    for_tier[node_idx] = conj_value_tiered;
                }
                for_tiered_metric.insert(tier_name.clone(), for_tier);
            }

            result.metrics.insert(metric_name.clone(), for_metric);
            if !for_tiered_metric.is_empty() {
                result
                    .tiered_metric
                    .insert(metric_name.clone(), for_tiered_metric);
            }
        }

        // make tiered metrics cumulative
        for metric_name in self.ag.metrics.keys() {
            if result.tiered_metric.is_empty() {
                break;
            }

            let tiered_for_metric = result
                .tiered_metric
                .get_mut(metric_name)
                .context("missing tiered for metric")?;

            for (tier_name, tier_idx) in &self.ag.state.tiers {
                if *tier_idx == 0 {
                    continue;
                }

                let prev_tier_metrics = tiered_for_metric
                    .get(&self.ag.state.tiers[tier_idx - 1].0)
                    .context("[conj cost] Missing tiered metric")?;

                let cml_metric_for_current_tier = tiered_for_metric
                    .get(tier_name)
                    .context("[conj cost get] Missing tiered metric")?
                    .iter()
                    .zip(prev_tier_metrics.iter())
                    .map(|(curr, prev)| curr + prev)
                    .collect();

                tiered_for_metric.insert(tier_name.clone(), cml_metric_for_current_tier);
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::TraversalConfig;
    use crate::tests::test_graphs::make_test_array_graph_2;
    use crate::tests::test_utils::traversal_config_test_trait::TraversalConfigTestTrait;

    #[test]
    fn test_conjoint_cost() -> Result<()> {
        let ag = make_test_array_graph_2()?.append_super_root(false)?;

        snapshot!(
            print_conj_cost(&ag),
            r#"
A
. Count: 6.75
. Metrics:
  - size: 6.75
B
. Count: 3.5
. Metrics:
  - size: 3.5
C
. Count: 1
. Metrics:
  - size: 1
D
. Count: 2.25
. Metrics:
  - size: 2.25
E
. Count: 1.5
. Metrics:
  - size: 1.5
F
. Count: 2
. Metrics:
  - size: 2
G
. Count: 1
. Metrics:
  - size: 1
H
. Count: 1
. Metrics:
  - size: 1
I
. Count: 1
. Metrics:
  - size: 1
J
. Count: 1.5
. Metrics:
  - size: 1.5
K
. Count: 0.5
. Metrics:
  - size: 0.5
L
. Count: 9.25
. Metrics:
  - size: 9.25
M
. Count: 6
. Metrics:
  - size: 6
N
. Count: 6
. Metrics:
  - size: 6
O
. Count: 6
. Metrics:
  - size: 6
P
. Count: 1
. Metrics:
  - size: 1
\u{10ffff}__root__\u{10ffff}
. Count: 17
. Metrics:
  - size: 16

"#
        );

        Ok(())
    }

    #[test]
    fn conj_cost_with_tiers() -> Result<()> {
        let mut ag = make_test_array_graph_2()?.append_super_root(false)?;
        let mut tvc = TraversalConfig::default();
        tvc.with_tier_config();

        ag.apply_traversal_config(tvc)?;
        snapshot!(
            print_conj_cost(&ag),
            r#"
A
. Count: 6.75
. Metrics:
  - size: 6.75
    - Tiered:
      - T1: 3.5
      - T2: 4.75
      - T3: 5.75
      - T4: 6.75
B
. Count: 3.5
. Metrics:
  - size: 3.5
    - Tiered:
      - T1: 1
      - T2: 1.5
      - T3: 2.5
      - T4: 3.5
C
. Count: 1
. Metrics:
  - size: 1
    - Tiered:
      - T1: 0
      - T2: 0
      - T3: 0
      - T4: 1
D
. Count: 2.25
. Metrics:
  - size: 2.25
    - Tiered:
      - T1: 1.5
      - T2: 2.25
      - T3: 2.25
      - T4: 2.25
E
. Count: 1.5
. Metrics:
  - size: 1.5
    - Tiered:
      - T1: 0
      - T2: 1.5
      - T3: 1.5
      - T4: 1.5
F
. Count: 2
. Metrics:
  - size: 2
    - Tiered:
      - T1: 2
      - T2: 2
      - T3: 2
      - T4: 2
G
. Count: 1
. Metrics:
  - size: 1
    - Tiered:
      - T1: 1
      - T2: 1
      - T3: 1
      - T4: 1
H
. Count: 1
. Metrics:
  - size: 1
    - Tiered:
      - T1: 1
      - T2: 1
      - T3: 1
      - T4: 1
I
. Count: 1
. Metrics:
  - size: 1
    - Tiered:
      - T1: 1
      - T2: 1
      - T3: 1
      - T4: 1
J
. Count: 1.5
. Metrics:
  - size: 1.5
    - Tiered:
      - T1: 0
      - T2: 0.5
      - T3: 1.5
      - T4: 1.5
K
. Count: 0.5
. Metrics:
  - size: 0.5
    - Tiered:
      - T1: 0
      - T2: 0.5
      - T3: 0.5
      - T4: 0.5
L
. Count: 9.25
. Metrics:
  - size: 9.25
    - Tiered:
      - T1: 8.5
      - T2: 9.25
      - T3: 9.25
      - T4: 9.25
M
. Count: 6
. Metrics:
  - size: 6
    - Tiered:
      - T1: 6
      - T2: 6
      - T3: 6
      - T4: 6
N
. Count: 6
. Metrics:
  - size: 6
    - Tiered:
      - T1: 6
      - T2: 6
      - T3: 6
      - T4: 6
O
. Count: 6
. Metrics:
  - size: 6
    - Tiered:
      - T1: 6
      - T2: 6
      - T3: 6
      - T4: 6
P
. Count: 1
. Metrics:
  - size: 1
    - Tiered:
      - T1: 1
      - T2: 1
      - T3: 1
      - T4: 1
\u{10ffff}__root__\u{10ffff}
. Count: 17
. Metrics:
  - size: 16
    - Tiered:
      - T1: 12
      - T2: 14
      - T3: 15
      - T4: 16

"#
        );
        Ok(())
    }

    fn print_conj_cost(ag: &ArrayGraph) -> String {
        let conj = ag.conjoint_cost();

        let mut result = String::new();

        for node_idx in ag.node_idx_iter() {
            result.push_str(&format!("{}\n", ag.idx_to_name(node_idx)));
            result.push_str(&format!(". Count: {}\n", conj.count[node_idx]));
            result.push_str(". Metrics:\n");
            for (metric_name, values) in &conj.metrics {
                result.push_str(&format!("  - {}: {}\n", metric_name, values[node_idx]));

                if let Some(tiered) = conj.tiered_metric.get(metric_name) {
                    result.push_str("    - Tiered:\n");
                    for (tier_name, tier_values) in tiered {
                        result.push_str(&format!(
                            "      - {}: {}\n",
                            tier_name, tier_values[node_idx]
                        ));
                    }
                }
            }
        }

        result
    }
}
