// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Matching nodes by their string properties.
//!
//! Properties are stored as an inverted index: `node_metadata.properties[name]`
//! maps straight to the nodes carrying that property and their values. Matching
//! therefore never walks the whole graph — it touches only the indices for the
//! property names actually asked about, and intersecting several conditions
//! seeds from the smallest of them so the candidate set starts as narrow as
//! possible.
//!
//! Shared by node search, ancestor search, and the tree table's filtered
//! entry points.

use std::collections::BTreeMap;

use crate::NodeIDX;
use crate::types::PropertyValue;
use crate::types::array_graph::ArrayGraph;

/// A set of property conditions bound to the graph's inverted indices.
pub struct PropertyIndices<'a>(Vec<BoundCondition<'a>>);

impl<'a> PropertyIndices<'a> {
    /// Bind each requested property name to its index in `ag`.
    ///
    /// An `expected_value` of `None` matches any node carrying the property,
    /// whatever its value.
    ///
    /// Returns `None` when a requested property name is absent from the graph
    /// entirely. No node can satisfy every condition in that case, so callers
    /// short-circuit to an empty result rather than silently dropping the
    /// unsatisfiable condition and over-matching.
    pub fn bind<I>(ag: &'a ArrayGraph, conditions: I) -> Option<Self>
    where
        I: IntoIterator<Item = (&'a str, Option<&'a str>)>,
    {
        conditions
            .into_iter()
            .map(|(name, expected_value)| {
                let index = ag.data.node_metadata.properties.get(name)?;
                Some(BoundCondition {
                    expected_value,
                    index,
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(Self)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether a single node satisfies every condition.
    pub fn matches(&self, node_idx: NodeIDX) -> bool {
        self.0.iter().all(|condition| condition.matches(node_idx))
    }

    /// Every node satisfying every condition, ascending.
    pub fn intersect(&self) -> Vec<NodeIDX> {
        let mut by_size: Vec<&BoundCondition<'a>> = self.0.iter().collect();
        by_size.sort_by_key(|condition| condition.index.len());

        let Some((seed, rest)) = by_size.split_first() else {
            return Vec::new();
        };

        let mut candidates = seed.matching_nodes();
        for condition in rest {
            candidates.retain(|&node_idx| condition.matches(node_idx));
        }
        candidates
    }
}

/// One property condition paired with the index it will be checked against.
struct BoundCondition<'a> {
    /// `None` matches any value, i.e. "the node has this property at all".
    expected_value: Option<&'a str>,
    index: &'a BTreeMap<NodeIDX, PropertyValue>,
}

impl BoundCondition<'_> {
    fn matches(&self, node_idx: NodeIDX) -> bool {
        self.index
            .get(&node_idx)
            .is_some_and(|actual| self.value_matches(actual))
    }

    /// Ascending, because `BTreeMap` iterates in key order — callers rely on
    /// this to skip a sort of the intersected candidate set.
    fn matching_nodes(&self) -> Vec<NodeIDX> {
        self.index
            .iter()
            .filter(|(_, value)| self.value_matches(value))
            .map(|(&node_idx, _)| node_idx)
            .collect()
    }

    fn value_matches(&self, actual: &str) -> bool {
        match self.expected_value {
            Some(expected) => actual == expected,
            None => true,
        }
    }
}
