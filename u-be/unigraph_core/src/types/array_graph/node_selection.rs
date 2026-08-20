// Copyright (c) Meta Platforms, Inc. and affiliates.

//! A declarative predicate over nodes: name, properties, edge tags.
//!
//! Not to be confused with the two interactive "selection" concepts in the UI —
//! the canvas box/lasso (`Selection` / `SelectionType`) and the set of nodes the
//! user has clicked (`SelectedNodesContext`). A [`NodeSelection`] describes
//! *which nodes match*, not which nodes someone picked.
//!
//! Evaluated by [`crate::types::array_graph::select_nodes`], and shared by the
//! tree table's filtered flat list, the `SearchNodes` RPC, and the
//! `ExploreGraph` / `ExploreDelta` `Matching` target.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::types::DynamicTypeKey;
use crate::types::PropertyName;
use crate::types::PropertyValue;
use crate::types::Tag;

/// Conditions that narrow the graph down to a subset of nodes.
///
/// A node matches only when it satisfies every condition — this is an AND
/// across all the fields and across the entries within each of them.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Default,
    PartialEq,
    unigraph_delta::Deltable
)]
pub struct NodeSelection {
    /// Node name must match this. Absent — or blank — matches every name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<NameMatch>,

    /// Property name -> what the value has to look like.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<PropertyName, PropertyValueMatch>,

    /// Node must have an incoming edge tagged with each of these.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub incoming_tags: BTreeSet<Tag>,

    /// Node must have an incoming dynamic edge with each of these type keys.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub incoming_dynamic_type_keys: BTreeSet<DynamicTypeKey>,

    /// Node must have an outgoing edge tagged with each of these.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub outgoing_tags: BTreeSet<Tag>,

    /// Node must have an outgoing dynamic edge with each of these type keys.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub outgoing_dynamic_type_keys: BTreeSet<DynamicTypeKey>,
}

/// How a node-name pattern is read.
///
/// `Substring` and `Regex` are predicates — every node's name is tested against
/// them. `Exact` and `Fuzzy` are generators: they produce candidates directly
/// from the name list, which is why the evaluator can seed from them instead of
/// scanning. See the module docs on `select_nodes` for what that means for
/// ordering and for the interaction between `Fuzzy` and the other conditions.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    Default,
    PartialEq,
    unigraph_delta::Deltable
)]
#[deltable(replace)]
pub enum NameMatchMode {
    /// Plain text, matched case-insensitively anywhere in the name.
    #[default]
    Substring,
    /// Rust `regex` syntax, unanchored and case-sensitive — prefix with `(?i)`
    /// to fold case, `^`/`$` to anchor.
    Regex,
    /// Subsequence match, shortest name first — the typeahead's behaviour.
    ///
    /// Top-K by construction, so it always runs against a cap and can return a
    /// prefix of the real match set rather than all of it.
    Fuzzy,
    /// The one node whose name is exactly this.
    Exact,
}

/// What a node's name has to look like.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Default,
    PartialEq,
    unigraph_delta::Deltable
)]
pub struct NameMatch {
    pub pattern: String,
    pub mode: NameMatchMode,
}

impl NameMatch {
    /// A blank pattern is a condition the user started and abandoned, not one
    /// that matches nothing — treat it as absent everywhere.
    pub fn is_blank(&self) -> bool {
        self.pattern.trim().is_empty()
    }
}

/// What a property condition requires of a node's value for that property.
///
/// A struct rather than a bare `Option<PropertyValue>` because this is a map
/// value: `JSON.stringify` drops `undefined`, so an optional-valued map entry
/// would silently disappear on the way back from the UI. An empty object
/// survives the round trip and leaves room for future match modes.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Default,
    PartialEq,
    unigraph_delta::Deltable
)]
pub struct PropertyValueMatch {
    /// Required exact value. Absent matches any node carrying the property,
    /// whatever its value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<PropertyValue>,
}

/// One direction's worth of edge conditions, so the matching logic can be
/// written once and run over the reverse graph and then the forward one.
pub struct EdgeConditions<'a> {
    pub tags: &'a BTreeSet<Tag>,
    pub dynamic_type_keys: &'a BTreeSet<DynamicTypeKey>,
}

impl EdgeConditions<'_> {
    /// Nothing to check — the corresponding edge view is never built.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.dynamic_type_keys.is_empty()
    }
}

impl NodeSelection {
    /// Match on the node name alone.
    ///
    /// Every field is listed rather than spread from `Default` so the compiler
    /// flags this constructor when a new condition is added.
    pub fn by_name(pattern: impl Into<String>, mode: NameMatchMode) -> Self {
        Self {
            name: Some(NameMatch {
                pattern: pattern.into(),
                mode,
            }),
            properties: BTreeMap::new(),
            incoming_tags: BTreeSet::new(),
            incoming_dynamic_type_keys: BTreeSet::new(),
            outgoing_tags: BTreeSet::new(),
            outgoing_dynamic_type_keys: BTreeSet::new(),
        }
    }

    /// Match on a single property. A `value` of `None` matches any node
    /// carrying the property, whatever its value.
    pub fn by_property(name: impl Into<PropertyName>, value: Option<PropertyValue>) -> Self {
        Self {
            name: None,
            properties: BTreeMap::from([(name.into(), PropertyValueMatch { value })]),
            incoming_tags: BTreeSet::new(),
            incoming_dynamic_type_keys: BTreeSet::new(),
            outgoing_tags: BTreeSet::new(),
            outgoing_dynamic_type_keys: BTreeSet::new(),
        }
    }

    /// No conditions set — every node matches.
    pub fn is_empty(&self) -> bool {
        self.name_condition().is_none()
            && self.properties.is_empty()
            && self.incoming_edges().is_empty()
            && self.outgoing_edges().is_empty()
    }

    /// The name condition, if there is one worth applying.
    pub fn name_condition(&self) -> Option<&NameMatch> {
        self.name.as_ref().filter(|name| !name.is_blank())
    }

    /// Conditions on the edges pointing at a node.
    pub fn incoming_edges(&self) -> EdgeConditions<'_> {
        EdgeConditions {
            tags: &self.incoming_tags,
            dynamic_type_keys: &self.incoming_dynamic_type_keys,
        }
    }

    /// Conditions on the edges leaving a node.
    pub fn outgoing_edges(&self) -> EdgeConditions<'_> {
        EdgeConditions {
            tags: &self.outgoing_tags,
            dynamic_type_keys: &self.outgoing_dynamic_type_keys,
        }
    }
}
