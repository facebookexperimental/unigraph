// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Enumerating everything the flat-list filter UI can offer as a typeahead
//! choice: property names, their values, edge tags, and dynamic type keys.
//!
//! Derived purely from `ag.data`, which is immutable — deliberately ignoring
//! reachability and excluded edges. That keeps this graph-lifetime rather than
//! traversal-config-lifetime, so the frontend can compute it once and hold it.
//! The cost is that a choice may currently match nothing, which just yields an
//! empty list.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::EdgeMeta;
use crate::NodeIDX;
use crate::types::DynamicTypeKey;
use crate::types::PropertyName;
use crate::types::PropertyValue;
use crate::types::Tag;
use crate::types::array_graph::ArrayGraph;

/// Past this many distinct values a property is treated as freeform.
///
/// Property values are unbounded in cardinality — a `commit_hash`-style
/// property has one distinct value per node, and serializing millions of them
/// across the WASM boundary would hang the tab.
const MAX_DISTINCT_VALUES: usize = 256;

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq
)]
pub struct PropertyCandidates {
    pub name: PropertyName,

    /// Distinct values, ascending. Empty when `high_cardinality`.
    pub values: Vec<PropertyValue>,

    /// The property has more distinct values than the UI can usefully offer,
    /// so they were not collected and the input should accept freeform text.
    pub high_cardinality: bool,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq
)]
pub struct FilterCandidates {
    pub properties: Vec<PropertyCandidates>,
    pub tags: Vec<Tag>,
    pub dynamic_type_keys: Vec<DynamicTypeKey>,
}

/// Every choice the filter UI can offer, each list ascending.
pub fn filter_candidates(ag: &ArrayGraph) -> FilterCandidates {
    let (tags, dynamic_type_keys) = distinct_edge_kinds(ag);
    FilterCandidates {
        properties: property_candidates(ag),
        tags,
        dynamic_type_keys,
    }
}

fn property_candidates(ag: &ArrayGraph) -> Vec<PropertyCandidates> {
    ag.data
        .node_metadata
        .properties
        .iter()
        .map(|(name, index)| {
            let values = distinct_values(index);
            PropertyCandidates {
                name: name.clone(),
                high_cardinality: values.is_none(),
                values: values.unwrap_or_default(),
            }
        })
        .collect()
}

/// Distinct values for one property, ascending, or `None` once the cap is
/// exceeded — bailing out rather than finishing a scan we intend to discard.
fn distinct_values(index: &BTreeMap<NodeIDX, PropertyValue>) -> Option<Vec<PropertyValue>> {
    let mut values: BTreeSet<&str> = BTreeSet::new();
    for value in index.values() {
        values.insert(value.as_str());
        if values.len() > MAX_DISTINCT_VALUES {
            return None;
        }
    }
    Some(values.into_iter().map(str::to_string).collect())
}

/// Distinct tags and dynamic type keys, ascending.
///
/// Folds the flat metadata table rather than the edges: it holds one entry per
/// (node, tag) and per (node, type_key, edge_name, branch) group, making this
/// O(distinct groups) instead of O(edges).
fn distinct_edge_kinds(ag: &ArrayGraph) -> (Vec<Tag>, Vec<DynamicTypeKey>) {
    let mut tags: BTreeSet<&str> = BTreeSet::new();
    let mut dynamic_type_keys: BTreeSet<&str> = BTreeSet::new();

    for meta in &ag.data.edges.edge_metadata {
        match meta {
            EdgeMeta::Tagged { tag } => {
                tags.insert(tag.as_str());
            }
            EdgeMeta::Dynamic { type_key, .. } => {
                dynamic_type_keys.insert(type_key.as_str());
            }
        }
    }

    (
        tags.into_iter().map(str::to_string).collect(),
        dynamic_type_keys.into_iter().map(str::to_string).collect(),
    )
}
