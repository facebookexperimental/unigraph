// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashMap;

use anyhow::Result;

use crate::types::NodeIDX;
use crate::types::NodeName;

/// Ordered list of all node names in a graph.
/// Stored as a massive single string with offsets recorded for how
/// to find each node. The single string is here so we don't have
/// to much memory fragmentation and can allocate, deallocate the
/// whole thing in one chunk. Searching though a single string is
/// also faster because we can optimize for CPU cache hits and
/// SIMD instructions.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct NodeNamesOrdered {
    pub(super) node_names: String,
    offsets: Vec<usize>,
}

impl NodeNamesOrdered {
    pub fn from_parts(node_names: String, offsets: Vec<usize>) -> Self {
        Self {
            node_names,
            offsets,
        }
    }

    pub fn nodes_len(&self) -> usize {
        self.offsets.len() - 1
    }

    #[inline]
    pub fn idx_to_name<I>(&self, node_idx: I) -> &str
    where
        I: Into<usize> + Copy,
    {
        let idx: usize = node_idx.into();
        let start = self.offsets[idx];
        let end = self.offsets[idx + 1];
        &self.node_names[start..end]
    }

    pub fn iter_names(&self) -> NodeNamesOrderedNamesIter {
        NodeNamesOrderedNamesIter {
            node_names: self,
            idx: 0,
        }
    }

    /// Given a name of the node name return its IDX if exists.
    /// Since we store names ordered in a single string we can
    /// use binary search to find the name.
    /// This function is O(log n) in the number of nodes.
    /// Which is enough for searching hundreds of nodes at a time
    /// but it can get pretty slow if we run it agains the entire big
    /// graph with 1M+ nodes.
    pub fn name_to_idx_log(&self, name: &str) -> Option<NodeIDX> {
        let mut low = 0;
        let mut high = self.nodes_len();
        while low < high {
            let mid = (low + high) / 2;
            let mid_name = self.idx_to_name(NodeIDX::from(mid));
            if mid_name < name {
                low = mid + 1;
            } else if mid_name > name {
                high = mid;
            } else {
                return Some(NodeIDX::from(mid));
            }
        }
        None
    }

    pub fn append_node_name(&mut self, name: &str) -> Result<NodeIDX> {
        let names_count = self.nodes_len();

        if names_count > 0 {
            // ensure the new name is > than the last name
            let last_name = self.idx_to_name(NodeIDX::from(self.nodes_len() - 1));
            if last_name >= name {
                return Err(anyhow::anyhow!(
                    "Node names must be ordered incrementally.
You are trying to append a new node name '{name}' to a list of nodes containing '{names_count}' elements.
The last node name in the list is '{last_name}'.
Last name must be `<` the new name, which was not the case.
",                ));
            }
        }

        let idx = names_count;
        self.node_names.push_str(name);
        self.offsets.push(self.node_names.len());
        Ok(NodeIDX::from(idx))
    }
}
pub(crate) struct NodeNamesOrderedBuilder {}

impl NodeNamesOrderedBuilder {
    pub(crate) fn from_names<I: IntoIterator<Item = NodeName>>(
        node_names: I,
    ) -> (NodeNamesOrdered, HashMap<NodeName, NodeIDX>) {
        let mut node_names = node_names.into_iter().collect::<Vec<_>>();
        node_names.sort();
        let mut offsets = vec![0];
        let mut node_names_flat = String::new();
        let mut map = HashMap::new();
        for (idx, name) in node_names.into_iter().enumerate() {
            node_names_flat.push_str(&name);
            offsets.push(node_names_flat.len());
            map.insert(name, NodeIDX::from(idx));
        }
        (
            NodeNamesOrdered {
                node_names: node_names_flat,
                offsets,
            },
            map,
        )
    }
}

pub struct NodeNamesOrderedNamesIter<'a> {
    node_names: &'a NodeNamesOrdered,
    idx: usize,
}

impl<'a> Iterator for NodeNamesOrderedNamesIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.node_names.nodes_len() {
            return None;
        }
        let name = self.node_names.idx_to_name(NodeIDX::from(self.idx));
        self.idx += 1;
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use k9::assert_equal;
    use k9::snapshot;

    use super::*;

    #[test]
    fn test_appending() -> Result<()> {
        let mut nn = NodeNamesOrdered {
            node_names: String::new(),
            offsets: vec![0],
        };

        assert_equal!(nn.append_node_name("meow")?.0, 0);
        assert_equal!(nn.node_names, "meow");

        assert_equal!(nn.append_node_name("woof")?.0, 1);
        assert_equal!(nn.node_names, "meowwoof");

        let e = nn.append_node_name("abcd").unwrap_err();

        snapshot!(
            e.to_string(),
            r#"
Node names must be ordered incrementally.
You are trying to append a new node name 'abcd' to a list of nodes containing '2' elements.
The last node name in the list is 'woof'.
Last name must be `<` the new name, which was not the case.

"#
        );
        Ok(())
    }
}
