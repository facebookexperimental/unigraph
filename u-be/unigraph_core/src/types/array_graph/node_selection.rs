// Copyright (c) Meta Platforms, Inc. and affiliates.

//! A declarative predicate over nodes: name, properties, metrics, edge tags.
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

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

use crate::types::DynamicTypeKey;
use crate::types::MetricName;
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

    /// Metric name -> the value a node's metric has to equal.
    ///
    /// Compared as integers, never as floats. A node's `f64` is rounded the
    /// same way [`MetricFormat::format_value`] rounds it to choose an `Enum`
    /// label, so a condition asks "does this render as that variant?" rather
    /// than "is this bitwise equal to that float?". Those two agree for an enum
    /// and would not for a float equality test, which is the whole reason this
    /// is an `i64`: matching what the table shows is the only useful contract,
    /// and it takes float comparison out of the picture entirely.
    ///
    /// That makes it a poor fit for a continuous metric — equalling a byte
    /// count exactly is rarely the question — so this exists for the enum and
    /// boolean formats. A range predicate would be a separate condition.
    ///
    /// Unlike [`properties`](Self::properties) the value is not optional:
    /// metrics are stored densely and default to `0.0`, so every node "has"
    /// every metric and "carries this at all" is not a question worth asking.
    ///
    /// [`MetricFormat::format_value`]: crate::graph_settings::MetricFormat::format_value
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metrics: BTreeMap<MetricName, i64>,

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

/// Parse one `NAME=VALUE` metric condition for [`NodeSelection::metrics`].
///
/// Lives here rather than in a CLI because the rounding is part of the
/// comparison's contract, not of one front end's argument parsing: every caller
/// has to agree on it or `package=6` means different things in different tools.
///
/// A float spelling is accepted and rounded the way a stored value is, so
/// `package=6` and `package=6.0` are the same condition — otherwise which one
/// matched would depend on how the caller happened to type it. Unlike a
/// property, the value is required: metrics are dense, so a bare name would
/// match every node and say nothing.
pub fn parse_metric_condition(condition: &str) -> Result<(MetricName, i64)> {
    let (name, raw) = condition
        .split_once('=')
        .with_context(|| format!("expected NAME=VALUE in a metric condition, got {condition:?}"))?;

    if name.is_empty() {
        bail!("expected NAME=VALUE in a metric condition, got {condition:?}");
    }

    Ok((name.to_string(), parse_metric_value(raw, condition)?))
}

fn parse_metric_value(raw: &str, condition: &str) -> Result<i64> {
    if let Ok(value) = raw.trim().parse::<i64>() {
        return Ok(value);
    }

    let as_float: f64 = raw.trim().parse().with_context(|| {
        format!("metric condition {condition:?} needs a number on the right of the `=`")
    })?;
    if !as_float.is_finite() {
        bail!("metric condition {condition:?} needs a finite number, got {raw:?}");
    }

    Ok(as_float.round() as i64)
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
            metrics: BTreeMap::new(),
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
            metrics: BTreeMap::new(),
            incoming_tags: BTreeSet::new(),
            incoming_dynamic_type_keys: BTreeSet::new(),
            outgoing_tags: BTreeSet::new(),
            outgoing_dynamic_type_keys: BTreeSet::new(),
        }
    }

    /// Match on a single metric. See [`metrics`](Self::metrics) for how the
    /// value is compared.
    pub fn by_metric(name: impl Into<MetricName>, value: i64) -> Self {
        Self {
            name: None,
            properties: BTreeMap::new(),
            metrics: BTreeMap::from([(name.into(), value)]),
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
            && self.metrics.is_empty()
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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of the whole `i64` design: an enum variant is addressable by
    /// the number the table shows, however the caller spells it.
    #[test]
    fn a_metric_condition_reads_the_same_written_as_an_int_or_a_float() {
        let as_int = parse_metric_condition("package=6").expect("`6` is a value");
        let as_float = parse_metric_condition("package=6.0").expect("`6.0` is the same value");

        assert_eq!(as_int, ("package".to_string(), 6));
        assert_eq!(
            as_int, as_float,
            "how the caller typed the number must not change which nodes match"
        );
    }

    /// Matches `MetricFormat::format_value`, which rounds before looking the
    /// variant up — so a stored 1.4 renders as variant 1 and must match `=1`.
    #[test]
    fn a_fractional_metric_value_rounds_the_way_the_formatter_rounds() {
        assert_eq!(
            parse_metric_condition("m=1.4")
                .expect("a fraction parses")
                .1,
            1
        );
        assert_eq!(
            parse_metric_condition("m=1.6")
                .expect("a fraction parses")
                .1,
            2
        );
        assert_eq!(
            parse_metric_condition("m=-0.4").expect("negatives too").1,
            0
        );
    }

    /// A metric is dense, so a bare name would match every node and say
    /// nothing — that is a typo, not a query.
    #[test]
    fn a_metric_condition_without_a_value_is_rejected() {
        for condition in ["package", "=6", "package=", "package=prod"] {
            assert!(
                parse_metric_condition(condition).is_err(),
                "{condition:?} should not parse as a metric condition"
            );
        }
    }

    #[test]
    fn metrics_count_towards_a_selection_being_non_empty() {
        let mut selection = NodeSelection::default();
        assert!(selection.is_empty(), "nothing set yet");

        selection.metrics.insert("package".to_string(), 6);
        assert!(
            !selection.is_empty(),
            "a metric condition alone is a real selection"
        );
    }
}
