/**
 * Represents values for metrics for a set of nodes.
 * Not transitive, just aggregated for things like
 * "give me total size of all the nodes i just selected"
 */
export interface CombinedMetricsForNodes {
  metrics: { [key: string]: number };
  tiered_metrics: { [key: string]: { [key: string]: number } };
  node_count: number;
}