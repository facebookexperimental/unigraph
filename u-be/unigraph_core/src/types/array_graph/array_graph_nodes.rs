// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;

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
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct ArrayGraphNodes {
    pub(super) node_names: String,
    offsets: Vec<usize>,
}
#[readonly::make]

pub struct SharedArrayGraphNodes {
    pub(super) node_names: Arc<ArrayGraphNodes>,
    existence: Vec<NodeExistenceFlags>,
    side: GraphSide,
    // Precomputed. only nodes that exist in the side of the graph.
    #[readonly]
    pub nodes_len: usize,
    pub existing_node_idxes: OnceLock<Arc<Vec<NodeIDX>>>,
}

/// Enum that represents one of the sides of the twin graph, either left graph or right graph.
#[derive(Clone, Copy)]
pub enum GraphSide {
    Left = 0b0001,
    Right = 0b0010,
}

bitflags::bitflags! {
    /// Flags that represent whether a node does not exist in one of the
    /// sides of the twin graph.
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct NodeExistenceFlags: u32 {
        const NOT_IN_LEFT =  GraphSide::Left as u32;
        const NOT_IN_RIGHT = GraphSide::Right as u32;
        const IN_BOTH = 0b0000;
    }
}

impl ArrayGraphNodes {
    pub fn from_parts(node_names: String, offsets: Vec<usize>) -> Self {
        Self {
            node_names,
            offsets,
        }
    }

    pub fn combined_nodes_len(&self) -> usize {
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

    /// Iterator over all node idxs for both graphs, they might or might not exist in one of
    /// the graphs.
    fn combined_node_idx_iter(
        &self,
    ) -> std::iter::Map<std::ops::Range<usize>, fn(usize) -> NodeIDX> {
        (0..self.combined_nodes_len()).map(NodeIDX::from)
    }

    pub fn combined_node_names_iter(&self) -> impl Iterator<Item = &str> {
        self.combined_node_idx_iter()
            .map(|idx| self.idx_to_name(idx))
    }

    /// Given a name of the node name return its IDX if exists.
    /// Since we store names ordered in a single string we can
    /// use binary search to find the name.
    /// This function is O(log n) in the number of nodes.
    /// Which is enough for searching hundreds of nodes at a time
    /// but it can get pretty slow if we run it agains the entire big
    /// graph with 1M+ nodes.
    fn name_to_idx_log(&self, name: &str) -> Option<NodeIDX> {
        let mut low = 0;
        let mut high = self.combined_nodes_len();
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
        let names_count = self.combined_nodes_len();

        if names_count > 0 {
            // ensure the new name is > than the last name
            let last_name = self.idx_to_name(NodeIDX::from(self.combined_nodes_len() - 1));
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

impl SharedArrayGraphNodes {
    pub fn new_left_only(nodes: Arc<ArrayGraphNodes>) -> Self {
        let existence = vec![NodeExistenceFlags::IN_BOTH; nodes.combined_nodes_len()];
        let nodes_len = nodes.combined_nodes_len(); // all nodes exist in the left side
        Self {
            node_names: nodes,
            existence,
            side: GraphSide::Left,
            nodes_len,
            existing_node_idxes: OnceLock::new(),
        }
    }

    fn existing_node_idxes(&self) -> Arc<Vec<NodeIDX>> {
        self.existing_node_idxes
            .get_or_init(|| {
                Arc::new(
                    self.node_names
                        .combined_node_idx_iter()
                        .filter(|&idx| !self.existence[idx].does_not_exist_in(self.side))
                        .collect::<Vec<_>>(),
                )
            })
            .clone()
    }

    #[inline]
    pub fn idx_to_name<I>(&self, node_idx: I) -> &str
    where
        I: Into<NodeIDX> + Copy,
    {
        self.node_names.idx_to_name(node_idx.into())
    }

    pub fn iter_names(&self) -> NodeNamesOrderedNamesIter {
        NodeNamesOrderedNamesIter {
            node_names: &self.node_names,
            existence: &self.existence,
            side: self.side,
            idx: 0,
        }
    }

    pub fn node_idx_iter(&self) -> NodeIDXsArcIter {
        NodeIDXsArcIter {
            existing_node_idxes: self.existing_node_idxes(),
            current_idx: 0,
        }
    }

    #[inline(always)]
    pub fn name_to_idx_log(&self, name: &str) -> Option<NodeIDX> {
        self.node_names.name_to_idx_log(name)
    }
}

pub(crate) struct NodeNamesOrderedBuilder {}

impl NodeNamesOrderedBuilder {
    pub(crate) fn from_names<I: IntoIterator<Item = NodeName>>(
        node_names: I,
    ) -> (ArrayGraphNodes, HashMap<NodeName, NodeIDX>) {
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
            ArrayGraphNodes {
                node_names: node_names_flat,
                offsets,
            },
            map,
        )
    }
}

pub struct NodeNamesOrderedNamesIter<'a> {
    node_names: &'a ArrayGraphNodes,
    existence: &'a [NodeExistenceFlags],
    side: GraphSide,
    idx: usize,
}

impl<'a> Iterator for NodeNamesOrderedNamesIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.node_names.combined_nodes_len() {
            return None;
        }
        let existence = self.existence[self.idx];
        if existence.does_not_exist_in(self.side) {
            self.idx += 1;
            return self.next(); // skip nodes that do not exist in the current side
        }
        let name = self.node_names.idx_to_name(NodeIDX::from(self.idx));
        self.idx += 1;
        Some(name)
    }
}

pub struct NodeIDXsArcIter {
    existing_node_idxes: Arc<Vec<NodeIDX>>,
    current_idx: usize,
}

impl Iterator for NodeIDXsArcIter {
    type Item = NodeIDX;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_idx >= self.existing_node_idxes.len() {
            None
        } else {
            let node_idx = self.existing_node_idxes[self.current_idx];
            self.current_idx += 1;
            Some(node_idx)
        }
    }
}

impl NodeExistenceFlags {
    pub fn does_not_exist_in(self, side: GraphSide) -> bool {
        match side {
            GraphSide::Left => self.contains(NodeExistenceFlags::NOT_IN_LEFT),
            GraphSide::Right => self.contains(NodeExistenceFlags::NOT_IN_RIGHT),
        }
    }
}
#[cfg(test)]
mod tests {
    use k9::assert_equal;
    use k9::snapshot;

    use super::*;

    #[test]
    fn test_appending() -> Result<()> {
        let mut nn = ArrayGraphNodes {
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
