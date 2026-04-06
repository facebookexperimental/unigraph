// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BinaryHeap;

use anyhow::Result;
use fst::IntoStreamer;
use fst::Streamer;
use fst::automaton::Subsequence;
use rayon::prelude::*;

use crate::ArrayGraphNodes;
use crate::NodeIDX;

type FstSet = fst::Set<Vec<u8>>;

/// Cached FST sets for fast subsequence search.
/// Built once per ArrayGraphNodes, reused across queries.
#[derive(Clone)]
pub struct FstSets {
    sets: Vec<FstSet>,
}

impl FstSets {
    /// Build FST sets from sorted node names, one per CPU core.
    /// Node names in ArrayGraphNodes are already sorted, so each chunk
    /// is also sorted — exactly what fst::Set::from_iter requires.
    pub fn build(nodes: &ArrayGraphNodes) -> Result<Self> {
        let num_chunks = num_chunks();
        let total = nodes.combined_nodes_len();
        let chunk_size = total.div_ceil(num_chunks);

        let sets = (0..num_chunks)
            .into_par_iter()
            .map(|i| {
                let start = i * chunk_size;
                let end = (start + chunk_size).min(total);
                let iter = (start..end).map(|idx| nodes.idx_to_name(NodeIDX::from(idx)));
                let set = fst::Set::from_iter(iter)?;
                Ok(set)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { sets })
    }
}

/// Search a single FstSet with the Subsequence automaton.
/// Returns up to `limit` matches ordered by string length (shortest first).
fn fst_search(set: &FstSet, pattern: &str, limit: usize) -> Result<Vec<String>> {
    let matcher = Subsequence::new(pattern);
    let mut stream = set.search(matcher).into_stream();

    let mut heap: BinaryHeap<(usize, String)> = BinaryHeap::with_capacity(limit + 1);

    while let Some(key) = stream.next() {
        let len = key.len();
        let worst_len = heap.peek().map(|(l, _)| *l).unwrap_or(usize::MAX);

        if len >= worst_len && heap.len() >= limit {
            continue;
        }

        let name = String::from_utf8(key.to_vec())?;
        heap.push((len, name));

        if heap.len() > limit {
            heap.pop();
        }
    }

    Ok(heap
        .into_sorted_vec()
        .into_iter()
        .map(|(_, name)| name)
        .collect())
}

fn num_chunks() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Search cached FST sets for subsequence matches.
///
/// Case-sensitive, but much faster than regex on large graphs
/// (seconds vs tens of seconds on 25M+ nodes).
///
/// Returns up to `limit` matches ordered by string length (shortest first).
pub fn search_fuzzy_fst<'a>(
    nodes: &'a ArrayGraphNodes,
    fst_sets: &FstSets,
    pattern: &str,
    limit: usize,
) -> Result<Vec<(&'a str, NodeIDX)>> {
    if pattern.is_empty() {
        return Ok(vec![]);
    }

    let per_chunk_results: Vec<Vec<String>> = fst_sets
        .sets
        .par_iter()
        .map(|set| fst_search(set, pattern, limit))
        .collect::<Result<Vec<_>>>()?;

    let mut merged: Vec<String> = per_chunk_results.into_iter().flatten().collect();
    merged.sort_by_key(|s| s.len());
    merged.truncate(limit);

    Ok(merged
        .iter()
        .filter_map(|name| {
            nodes
                .name_to_idx_log(name)
                .map(|idx| (nodes.idx_to_name(idx), idx))
        })
        .collect())
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

    fn search_fst(nodes: &ArrayGraphNodes, query: &str, limit: usize) -> Result<String> {
        let fst_sets = FstSets::build(nodes)?;
        Ok(search_fuzzy_fst(nodes, &fst_sets, query, limit)?
            .into_iter()
            .map(|(name, _idx)| name.to_string())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    #[test]
    fn test_fst_search_basic() -> Result<()> {
        let nodes = create_test_graph_nodes();

        // FST is case-sensitive: "app" matches "application" but NOT "ApPlE" or "aPpLy"
        snapshot!(search_fst(&nodes, "app", 10)?, "application");

        Ok(())
    }

    #[test]
    fn test_fst_search_limit() -> Result<()> {
        let nodes = create_test_graph_nodes();

        // "a" (lowercase, case-sensitive) — shortest 2 matches
        snapshot!(
            search_fst(&nodes, "a", 2)?,
            "
aPpLy
grape
"
        );

        Ok(())
    }

    #[test]
    fn test_fst_search_no_matches() -> Result<()> {
        let nodes = create_test_graph_nodes();

        snapshot!(search_fst(&nodes, "zzz", 10)?, "");

        Ok(())
    }

    #[test]
    fn test_fst_search_empty_pattern() -> Result<()> {
        let nodes = create_test_graph_nodes();

        snapshot!(search_fst(&nodes, "", 10)?, "");

        Ok(())
    }

    #[test]
    fn test_fst_search_subsequence() -> Result<()> {
        let nodes = create_test_graph_nodes();

        // "ae" as subsequence (case-sensitive):
        // "grape" has 'a' then 'e', "//howdy/partner$^/meow" has 'a' in partner then 'e' in meow
        snapshot!(
            search_fst(&nodes, "ae", 10)?,
            "
grape
//howdy/partner$^/meow
"
        );

        Ok(())
    }

    #[test]
    fn test_fst_search_special_chars() -> Result<()> {
        let nodes = create_test_graph_nodes();

        // Special chars work directly — no regex escaping needed with FST
        snapshot!(search_fst(&nodes, "//h", 10)?, "//howdy/partner$^/meow");

        Ok(())
    }

    #[test]
    fn test_fst_search_case_sensitive() -> Result<()> {
        let nodes = create_test_graph_nodes();

        // "AP" matches "ApPlE" — 'A' at pos 0, 'P' at pos 2 (subsequence)
        snapshot!(search_fst(&nodes, "AP", 10)?, "ApPlE");

        // "Ap" also matches "ApPlE" — 'A' at pos 0, 'p' at pos 1
        snapshot!(search_fst(&nodes, "Ap", 10)?, "ApPlE");

        // "ap" (all lowercase) — matches anything with 'a' then 'p'
        snapshot!(
            search_fst(&nodes, "ap", 10)?,
            "
aPpLy
grape
application
"
        );

        Ok(())
    }

    #[test]
    fn test_fst_search_single_char() -> Result<()> {
        let nodes = create_test_graph_nodes();

        // lowercase 'a' — case-sensitive, so 'ApPlE' (capital A) does NOT match
        snapshot!(
            search_fst(&nodes, "a", 10)?,
            "
aPpLy
grape
banana
bandana
application
//howdy/partner$^/meow
"
        );

        Ok(())
    }

    #[test]
    fn test_fst_search_multiple_chunks() -> Result<()> {
        let nodes = create_test_graph_nodes();

        // Force 3 chunks to verify cross-chunk merging works
        let fst_sets = FstSets::build(&nodes)?;
        assert!(!fst_sets.sets.is_empty());

        // "an" subsequence: banana, bandana, application (a...n), //howdy/partner$^/meow (a...n)
        snapshot!(
            search_fst(&nodes, "an", 10)?,
            "
banana
bandana
application
//howdy/partner$^/meow
"
        );

        Ok(())
    }

    #[test]
    fn test_fst_sets_reuse() -> Result<()> {
        let nodes = create_test_graph_nodes();

        // Build once, search multiple times
        let fst_sets = FstSets::build(&nodes)?;

        let r1 = search_fuzzy_fst(&nodes, &fst_sets, "app", 10)?;
        let r2 = search_fuzzy_fst(&nodes, &fst_sets, "ban", 10)?;

        let names1: Vec<&str> = r1.iter().map(|(n, _)| *n).collect();
        let names2: Vec<&str> = r2.iter().map(|(n, _)| *n).collect();

        snapshot!(names1.join("\n"), "application");
        snapshot!(
            names2.join("\n"),
            "
banana
bandana
"
        );

        Ok(())
    }
}
