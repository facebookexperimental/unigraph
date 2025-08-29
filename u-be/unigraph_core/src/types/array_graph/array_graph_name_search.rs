// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BinaryHeap;

use anyhow::Result;

use crate::ArrayGraphNodes;
use crate::NodeIDX;

/// Search for nodes using regex pattern matching.
/// Iterates through all node names and matches them against the provided regex pattern.
/// Like search_fuzzy, this creates a subsequence pattern by escaping regex characters
/// and inserting .* between each character for fuzzy matching.
/// Returns up to `limit` matches ordered by string length (shortest first).
pub fn search_fuzzy_regex<'a>(
    nodes: &'a ArrayGraphNodes,
    pattern: &str,
    limit: usize,
) -> Result<Vec<(&'a str, NodeIDX)>> {
    // Create a case insensitive regex pattern that matches the query as a subsequence
    // For each character in the query, we want to match it anywhere in the string
    // with other characters potentially in between (subsequence matching)
    let pattern_parts: Vec<String> = pattern
        .chars()
        .map(|c| regex::escape(&c.to_string()))
        .collect();

    // Join the escaped characters with ".*" to allow any characters in between
    // This creates a subsequence match: for "ae" -> ".*a.*e.*"
    let pattern = format!(".*{}.*", pattern_parts.join(".*"));
    let regex = regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()?;

    // Binary heap so we can search for only N top results ordered
    // by node length ascending.
    //
    // Since it's a fuzzy search, it will likely match a LOT of garbage
    // that is likely irrelevant.
    // e.g. if you search for `cat`
    // it will match strings like
    // - *CAT*astrophiclly_terrible_match
    // - a*C*tu*A*lly_another_*T*errible_match
    // - cat
    // - also_a_cat
    // Obviously we want to surface `cat` and `also_a_cat` as first
    // matches. A good heuristic is to prioritize shorter names.
    let mut heap = BinaryHeap::new();

    for node_idx in nodes.combined_node_idx_iter() {
        let node_name = nodes.idx_to_name(node_idx);
        let len = node_name.len() as i32;
        let current_longest_match = heap.peek().map(|(len, _)| *len).unwrap_or(i32::MAX);

        if len >= current_longest_match && heap.len() >= limit {
            // This match is longer than the longest match we have
            // and we already have enough matches, so skip it instead matching, pushing and popping
            continue;
        }

        if regex.is_match(node_name) {
            heap.push((len, node_idx));

            if heap.len() > limit {
                heap.pop();
            }
        }
    }

    Ok(heap
        .into_sorted_vec()
        .into_iter()
        .map(|(_len, node_idx)| (nodes.idx_to_name(node_idx), node_idx))
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
