// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BinaryHeap;
use std::sync::OnceLock;

use anyhow::Result;
use rayon::prelude::*;

use super::array_graph_fst_search::FstSets;
use super::array_graph_fst_search::search_fuzzy_fst;
use crate::ArrayGraphNodes;
use crate::NodeIDX;

/// Graphs above this threshold use FST-based search (case-sensitive, much faster).
/// Graphs below use regex-based search (case-insensitive).
const FST_SEARCH_THRESHOLD: usize = 3_000_000;

/// Encapsulates fuzzy name search with automatic strategy selection.
///
/// - Small graphs (< 3M nodes): regex-based subsequence search (case-insensitive)
/// - Large graphs (>= 3M nodes): FST-based subsequence search (case-sensitive, cached)
///
/// FST sets are built lazily on first search and reused for subsequent queries.
#[derive(Default, Clone)]
pub struct NameSearch {
    fst_sets: OnceLock<FstSets>,
}

impl NameSearch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fuzzy-search node names, returning up to `limit` matches
    /// ordered by string length (shortest first).
    pub fn search<'a>(
        &self,
        nodes: &'a ArrayGraphNodes,
        pattern: &str,
        limit: usize,
    ) -> Result<Vec<(&'a str, NodeIDX)>> {
        if nodes.combined_nodes_len() >= FST_SEARCH_THRESHOLD {
            let fst_sets = self.fst_sets.get_or_try_init(|| FstSets::build(nodes))?;
            search_fuzzy_fst(nodes, fst_sets, pattern, limit)
        } else {
            search_fuzzy_regex(nodes, pattern, limit)
        }
    }
}

/// Fuzzy-search node names using regex subsequence matching, parallelized with rayon.
///
/// Each rayon thread maintains a local top-K heap (bounded by `limit`), then heaps
/// are merged pairwise. Memory usage is O(limit * num_threads), not O(total_matches).
///
/// On WASM (no threads), rayon automatically falls back to sequential execution.
fn search_fuzzy_regex<'a>(
    nodes: &'a ArrayGraphNodes,
    pattern: &str,
    limit: usize,
) -> Result<Vec<(&'a str, NodeIDX)>> {
    let regex = build_subsequence_regex(pattern)?;
    let heap = collect_top_k_matches(nodes, &regex, limit);
    Ok(sorted_results(nodes, heap))
}

fn build_subsequence_regex(pattern: &str) -> Result<regex::Regex> {
    let pattern_parts: Vec<String> = pattern
        .chars()
        .map(|c| regex::escape(&c.to_string()))
        .collect();

    let pattern = format!(".*{}.*", pattern_parts.join(".*"));
    Ok(regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()?)
}

/// Parallel scan: each thread keeps a local BinaryHeap of at most `limit` entries
/// (max-heap by name length), then heaps are merged pairwise via reduce.
fn collect_top_k_matches(
    nodes: &ArrayGraphNodes,
    regex: &regex::Regex,
    limit: usize,
) -> BinaryHeap<(i32, NodeIDX)> {
    nodes
        .combined_node_idx_iter()
        .par_bridge()
        .fold(
            || BinaryHeap::with_capacity(limit + 1),
            |mut heap, node_idx| {
                let node_name = nodes.idx_to_name(node_idx);
                let len = node_name.len() as i32;

                // Skip the expensive regex check for names that can't improve the result.
                // Uses >= (not >) intentionally: equal-length names won't shorten the top-K,
                // so we avoid the regex cost. This means tiebreaking among same-length names
                // is non-deterministic across threads, which is acceptable.
                if len >= heap.peek().map(|(l, _)| *l).unwrap_or(i32::MAX) && heap.len() >= limit {
                    return heap;
                }

                if regex.is_match(node_name) {
                    heap.push((len, node_idx));
                    if heap.len() > limit {
                        heap.pop();
                    }
                }

                heap
            },
        )
        .reduce(
            || BinaryHeap::with_capacity(limit + 1),
            |a, b| merge_heaps(a, b, limit),
        )
}

fn merge_heaps(
    mut a: BinaryHeap<(i32, NodeIDX)>,
    b: BinaryHeap<(i32, NodeIDX)>,
    limit: usize,
) -> BinaryHeap<(i32, NodeIDX)> {
    for item in b {
        a.push(item);
        if a.len() > limit {
            a.pop();
        }
    }
    a
}

fn sorted_results<'a>(
    nodes: &'a ArrayGraphNodes,
    heap: BinaryHeap<(i32, NodeIDX)>,
) -> Vec<(&'a str, NodeIDX)> {
    heap.into_sorted_vec()
        .into_iter()
        .map(|(_len, node_idx)| (nodes.idx_to_name(node_idx), node_idx))
        .collect()
}

#[cfg(test)]
mod tests {

    use k9::snapshot;

    use super::*;
    use crate::ArrayGraphNodes;
    use crate::types::array_graph::array_graph_nodes::NodeNamesOrderedBuilder;

    fn create_test_graph_nodes() -> ArrayGraphNodes {
        let node_names = vec![
            "ApPlE".to_string(),
            "application".to_string(),
            "aPpLy".to_string(),
            "banana".to_string(),
            "bandana".to_string(),
            "cherry".to_string(),
            "grape".to_string(),
            "//howdy/partner$^/meow".to_string(),
        ];

        let (array_nodes, _) = NodeNamesOrderedBuilder::from_names(node_names);

        array_nodes
    }

    fn search_fuzzy(nodes: &ArrayGraphNodes, query: &str, limit: usize) -> Result<String> {
        Ok(nodes
            .search_name_fuzzy(query, limit)?
            .into_iter()
            .map(|(name, _idx)| name.to_string())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    #[test]
    fn test_fuzzy_search() -> Result<()> {
        let nodes = create_test_graph_nodes();

        snapshot!(
            search_fuzzy(&nodes, "app", 10)?,
            "
ApPlE
aPpLy
application
"
        );

        snapshot!(search_fuzzy(&nodes, "app", 1)?, "ApPlE");
        snapshot!(search_fuzzy(&nodes, "zzz", 1)?, "");
        snapshot!(
            search_fuzzy(&nodes, "a", 10)?,
            "
ApPlE
aPpLy
grape
banana
bandana
application
//howdy/partner$^/meow
"
        );

        snapshot!(search_fuzzy(&nodes, "ae", 1)?, "ApPlE");

        // Test fuzzy subsequence matching
        snapshot!(
            search_fuzzy(&nodes, "ae", 10)?,
            "
ApPlE
grape
//howdy/partner$^/meow
"
        );

        snapshot!(search_fuzzy(&nodes, "/$W", 10)?, "//howdy/partner$^/meow");

        Ok(())
    }
}
