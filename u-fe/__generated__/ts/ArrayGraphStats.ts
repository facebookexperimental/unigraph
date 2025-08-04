export interface ArrayGraphStats {
  num_all_nodes: number;
  num_all_edges: number;
  num_directed_edges: number;
  num_tagged_edges: number;
  num_dynamic_edges: number;
  num_unreachable_nodes: number;
  num_excluded_edges: number;
  tier_names: string[];
}