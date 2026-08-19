// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::fmt;
use std::str::FromStr;

use anyhow::bail;

const SEPARATOR: char = '~';
const TIER_SEPARATOR: char = '#';
const SIDE_SEPARATOR: char = '@';
const NODE_COUNT: &str = "node-count";
const PARENTS_COUNT: &str = "parents-count";
const TIER_INDEX: &str = "tier";
const TRANSITIVE: &str = "transitive";
const DOMINATED: &str = "dominated";
const LEFT: &str = "left";
const DELTA: &str = "delta";
/// Never emitted — the primary graph has no suffix. Accepted because
/// `SortColumn`'s docs advertised this spelling while its key was an untyped
/// `String`.
const RIGHT_ALIAS: &str = "right";

/// An override of *which* graph a metric view reads from, when a table is
/// comparing two.
///
/// Deliberately has no `Right` variant: a view's side is optional, and its
/// absence means the primary ("after") graph — the only graph outside delta
/// mode. So the common case has exactly one representation. Single-graph code
/// leaves the side unset, JSON omits the field entirely, and there is no
/// second spelling that would render identically yet compare unequal in the
/// maps these views key.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen
)]
pub enum MetricSide {
    /// The "before" graph.
    Left,
    /// Right minus left. Not always a plain subtraction: tiered and node-count
    /// deltas exclude nodes that didn't change.
    Delta,
}

/// A user-facing metric specification.
///
/// Describes which metric to compute for a node. Not the raw data itself,
/// but the *view* — plain value, transitive sum, dominated sum, tiered, or
/// a structural count like parent count or transitive node count.
///
/// ## Sides
///
/// Every view carries an optional [`MetricSide`]. `None` — the single-graph
/// case, and what JSON gets when the field is simply omitted — means the
/// primary graph; `Some(Left)` / `Some(Delta)` only mean anything when a table
/// is comparing two. Keeping it here rather than in a separate wrapper type
/// means one vocabulary covers both modes: sort keys, RPC column lists, and
/// the metrics map are all just `MetricView`.
///
/// ## String format
///
/// `MetricView` implements `Display` and `FromStr`. `~` separates the metric
/// name from the view variant, `#` introduces a tier name, and `@` the side:
///
/// ```text
/// size                  → Metric { name: "size" }
/// size~transitive       → Transitive { name: "size" }
/// size~dominated        → Dominated { name: "size" }
/// size#T1               → Tiered { name: "size", tier_name: "T1" }
/// size#T1~dominated     → TieredDominated { name: "size", tier_name: "T1" }
/// node-count~transitive → CountTransitive
/// node-count~dominated  → CountDominated
/// parents-count         → ParentsCount
/// tier                  → TierIndex
///
/// size~transitive@left  → Transitive { name: "size", side: Some(Left) }
/// size#T1@delta         → Tiered { name: "size", tier_name: "T1", side: Some(Delta) }
/// ```
///
/// ## Legacy forms
///
/// Tier names were once introduced by `~` rather than `#`. Those keys are
/// persisted inside `GraphSettings` on stored graphs, so both spellings parse;
/// `Display` always emits the current one, which migrates a key on rewrite.
///
/// ```text
/// size~T1               → Tiered { name: "size", tier_name: "T1" }
/// size~dominated~T1     → TieredDominated { name: "size", tier_name: "T1" }
/// ```
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen
)]
pub enum MetricView {
    /// Raw metric value (e.g. file size in bytes).
    Metric {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
    /// Transitive metric sum (DFS over forward edges).
    Transitive {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
    /// Dominated metric sum (DFS over dominator tree).
    Dominated {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
    /// Tiered transitive metric (cumulative at a specific tier).
    Tiered {
        name: String,
        tier_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
    /// Tiered dominated metric (dominated sum at a specific tier).
    TieredDominated {
        name: String,
        tier_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
    /// Number of configured parents (incoming edges).
    ParentsCount {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
    /// Transitive dependency count (forward DFS).
    CountTransitive {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
    /// Dominated dependency count (dominator tree DFS).
    CountDominated {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
    /// Tier index of the node (0-based). Only available when tiers are configured.
    TierIndex {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<MetricSide>,
    },
}

// ── Constructors ────────────────────────────────────────────────
// All produce side-less views: that is what a single graph yields, and delta
// columns are derived from them with `with_side`.

impl MetricView {
    pub fn metric(name: impl Into<String>) -> Self {
        Self::Metric {
            name: name.into(),
            side: None,
        }
    }

    pub fn transitive(name: impl Into<String>) -> Self {
        Self::Transitive {
            name: name.into(),
            side: None,
        }
    }

    pub fn dominated(name: impl Into<String>) -> Self {
        Self::Dominated {
            name: name.into(),
            side: None,
        }
    }

    pub fn tiered(name: impl Into<String>, tier_name: impl Into<String>) -> Self {
        Self::Tiered {
            name: name.into(),
            tier_name: tier_name.into(),
            side: None,
        }
    }

    pub fn tiered_dominated(name: impl Into<String>, tier_name: impl Into<String>) -> Self {
        Self::TieredDominated {
            name: name.into(),
            tier_name: tier_name.into(),
            side: None,
        }
    }

    pub fn parents_count() -> Self {
        Self::ParentsCount { side: None }
    }

    pub fn count_transitive() -> Self {
        Self::CountTransitive { side: None }
    }

    pub fn count_dominated() -> Self {
        Self::CountDominated { side: None }
    }

    pub fn tier_index() -> Self {
        Self::TierIndex { side: None }
    }
}

// ── Accessors ───────────────────────────────────────────────────

impl MetricView {
    /// The source metric name, if this view is derived from a named metric.
    /// Returns `None` for structural counts (ParentsCount, CountTransitive, CountDominated).
    pub fn metric_name(&self) -> Option<&str> {
        match self {
            MetricView::Metric { name, .. }
            | MetricView::Transitive { name, .. }
            | MetricView::Dominated { name, .. }
            | MetricView::Tiered { name, .. }
            | MetricView::TieredDominated { name, .. } => Some(name),
            MetricView::ParentsCount { .. }
            | MetricView::CountTransitive { .. }
            | MetricView::CountDominated { .. }
            | MetricView::TierIndex { .. } => None,
        }
    }

    pub fn is_dominated(&self) -> bool {
        matches!(
            self,
            MetricView::Dominated { .. }
                | MetricView::TieredDominated { .. }
                | MetricView::CountDominated { .. }
        )
    }

    /// `None` means the primary graph — the only graph outside delta mode.
    pub fn side(&self) -> Option<MetricSide> {
        match self {
            MetricView::Metric { side, .. }
            | MetricView::Transitive { side, .. }
            | MetricView::Dominated { side, .. }
            | MetricView::Tiered { side, .. }
            | MetricView::TieredDominated { side, .. }
            | MetricView::ParentsCount { side }
            | MetricView::CountTransitive { side }
            | MetricView::CountDominated { side }
            | MetricView::TierIndex { side } => *side,
        }
    }

    pub fn is_delta(&self) -> bool {
        self.side() == Some(MetricSide::Delta)
    }

    /// The same view read from a different graph. `None` restores the primary
    /// graph, so this doubles as [`Self::base`].
    pub fn with_side(&self, new_side: Option<MetricSide>) -> Self {
        let mut view = self.clone();
        match &mut view {
            MetricView::Metric { side, .. }
            | MetricView::Transitive { side, .. }
            | MetricView::Dominated { side, .. }
            | MetricView::Tiered { side, .. }
            | MetricView::TieredDominated { side, .. }
            | MetricView::ParentsCount { side }
            | MetricView::CountTransitive { side }
            | MetricView::CountDominated { side }
            | MetricView::TierIndex { side } => *side = new_side,
        }
        view
    }

    /// This view with the side dropped.
    ///
    /// Use it whenever a view is looked up in something side-agnostic — the
    /// `metrics_visibility` map and `MetricsConfig` are keyed by *what* is
    /// measured, so `size~transitive@left` must resolve like `size~transitive`.
    pub fn base(&self) -> Self {
        self.with_side(None)
    }
}

// ── String form ─────────────────────────────────────────────────

impl fmt::Display for MetricView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricView::Metric { name, .. } => write!(f, "{name}")?,
            MetricView::Transitive { name, .. } => write!(f, "{name}{SEPARATOR}{TRANSITIVE}")?,
            MetricView::Dominated { name, .. } => write!(f, "{name}{SEPARATOR}{DOMINATED}")?,
            MetricView::Tiered {
                name, tier_name, ..
            } => write!(f, "{name}{TIER_SEPARATOR}{tier_name}")?,
            MetricView::TieredDominated {
                name, tier_name, ..
            } => write!(f, "{name}{TIER_SEPARATOR}{tier_name}{SEPARATOR}{DOMINATED}")?,
            MetricView::ParentsCount { .. } => write!(f, "{PARENTS_COUNT}")?,
            MetricView::CountTransitive { .. } => write!(f, "{NODE_COUNT}{SEPARATOR}{TRANSITIVE}")?,
            MetricView::CountDominated { .. } => write!(f, "{NODE_COUNT}{SEPARATOR}{DOMINATED}")?,
            MetricView::TierIndex { .. } => write!(f, "{TIER_INDEX}")?,
        }

        match self.side() {
            None => Ok(()),
            Some(MetricSide::Left) => write!(f, "{SIDE_SEPARATOR}{LEFT}"),
            Some(MetricSide::Delta) => write!(f, "{SIDE_SEPARATOR}{DELTA}"),
        }
    }
}

impl FromStr for MetricView {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        let (body, side) = split_side(s)?;
        Ok(parse_view(body)?.with_side(side))
    }
}

fn split_side(s: &str) -> anyhow::Result<(&str, Option<MetricSide>)> {
    let Some((body, side)) = s.split_once(SIDE_SEPARATOR) else {
        return Ok((s, None));
    };
    let side = match side {
        LEFT => Some(MetricSide::Left),
        DELTA => Some(MetricSide::Delta),
        // The primary graph has no suffix; `@right` is only accepted because
        // `SortColumn`'s docs once advertised it.
        RIGHT_ALIAS => None,
        other => bail!("unknown metric side: '{other}' (expected '{LEFT}' or '{DELTA}')"),
    };
    Ok((body, side))
}

fn parse_view(s: &str) -> anyhow::Result<MetricView> {
    // Split on '#' first to separate metric name from tier info.
    if let Some((metric_part, tier_part)) = s.split_once(TIER_SEPARATOR) {
        return parse_tiered(metric_part, tier_part);
    }
    parse_non_tiered(s)
}

/// Parse a tiered metric view: `name#tier` or `name#tier~modifier`.
fn parse_tiered(name: &str, tier_part: &str) -> anyhow::Result<MetricView> {
    match tier_part.split_once(SEPARATOR) {
        None => Ok(MetricView::tiered(name, tier_part)),
        Some((tier_name, DOMINATED)) => Ok(MetricView::tiered_dominated(name, tier_name)),
        Some((_, modifier)) => {
            bail!("unknown tiered modifier: '{modifier}' (expected 'dominated')")
        }
    }
}

/// Parse a non-tiered metric view (no `#` present).
///
/// The last two arms are the legacy `~`-separated tier spellings. They must
/// stay below the `transitive` / `dominated` arms so those keep their meaning,
/// and they are why `size~T2` — written by a build of Unigraph that predates
/// the `#` separator — still loads.
fn parse_non_tiered(s: &str) -> anyhow::Result<MetricView> {
    let parts: Vec<&str> = s.split(SEPARATOR).collect();
    match parts.as_slice() {
        [PARENTS_COUNT] => Ok(MetricView::parents_count()),
        [TIER_INDEX] => Ok(MetricView::tier_index()),
        [NODE_COUNT, TRANSITIVE] => Ok(MetricView::count_transitive()),
        [NODE_COUNT, DOMINATED] => Ok(MetricView::count_dominated()),
        [NODE_COUNT, other] => {
            bail!("unknown node-count variant: '{other}' (expected 'transitive' or 'dominated')")
        }
        [name] => Ok(MetricView::metric(*name)),
        [name, TRANSITIVE] => Ok(MetricView::transitive(*name)),
        [name, DOMINATED] => Ok(MetricView::dominated(*name)),
        [name, DOMINATED, tier_name] => Ok(MetricView::tiered_dominated(*name, *tier_name)),
        [name, tier_name] => Ok(MetricView::tiered(*name, *tier_name)),
        _ => bail!("invalid metric view: '{s}'"),
    }
}

/// serde codec for fields that persist a [`MetricView`] as its display string
/// rather than as a struct — notably `SortColumn::MetricView.key`, whose keys
/// are already on disk in that form.
pub mod as_string {
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serializer;

    use super::MetricView;

    pub fn serialize<S: Serializer>(view: &MetricView, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(view)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<MetricView, D::Error> {
        let raw = String::deserialize(d)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;

    fn all_variants() -> Vec<MetricView> {
        vec![
            MetricView::metric("size"),
            MetricView::transitive("size"),
            MetricView::dominated("size"),
            MetricView::tiered("size", "T1"),
            MetricView::tiered_dominated("size", "T1"),
            MetricView::parents_count(),
            MetricView::count_transitive(),
            MetricView::count_dominated(),
            MetricView::tier_index(),
        ]
    }

    /// Every variant × side, rendered and parsed back. One table so the whole
    /// column vocabulary — the strings that end up in `SortColumn`, in stored
    /// `GraphSettings`, and in the frontend's key comparisons — is visible in
    /// one place.
    #[test]
    fn test_display_parse_roundtrip() {
        let sides = [None, Some(MetricSide::Left), Some(MetricSide::Delta)];
        let mut out = format!("{:<32} {:<12} {}\n", "display", "metric_name", "is_delta");

        for view in all_variants() {
            for side in sides {
                let view = view.with_side(side);
                let display = view.to_string();
                let parsed: MetricView = display
                    .parse()
                    .unwrap_or_else(|e| panic!("roundtrip failed for '{display}': {e}"));
                assert_eq!(parsed, view, "roundtrip mismatch for '{display}'");
                out.push_str(&format!(
                    "{:<32} {:<12} {}\n",
                    display,
                    view.metric_name().unwrap_or("-"),
                    view.is_delta(),
                ));
            }
        }

        snapshot!(
            out,
            "
display                          metric_name  is_delta
size                             size         false
size@left                        size         false
size@delta                       size         true
size~transitive                  size         false
size~transitive@left             size         false
size~transitive@delta            size         true
size~dominated                   size         false
size~dominated@left              size         false
size~dominated@delta             size         true
size#T1                          size         false
size#T1@left                     size         false
size#T1@delta                    size         true
size#T1~dominated                size         false
size#T1~dominated@left           size         false
size#T1~dominated@delta          size         true
parents-count                    -            false
parents-count@left               -            false
parents-count@delta              -            true
node-count~transitive            -            false
node-count~transitive@left       -            false
node-count~transitive@delta      -            true
node-count~dominated             -            false
node-count~dominated@left        -            false
node-count~dominated@delta       -            true
tier                             -            false
tier@left                        -            false
tier@delta                       -            true

"
        );
    }

    /// A key written before the `#` tier separator existed. Real stored graphs
    /// carry these; failing to parse one would sink the whole `GraphSettings`.
    #[test]
    fn test_legacy_tier_separator_still_parses() {
        let cases = [
            "size~T2",
            "size~dominated~T2",
            "size#T2",
            "size#T2~dominated",
        ];
        let out = cases
            .iter()
            .map(|raw| {
                let parsed: MetricView = raw
                    .parse()
                    .unwrap_or_else(|e| panic!("failed to parse '{raw}': {e}"));
                // Display always emits the current spelling, so rewriting a
                // graph migrates the key.
                format!("{raw:<22} → {parsed}")
            })
            .collect::<Vec<_>>()
            .join("\n");

        snapshot!(
            out,
            "
size~T2                → size#T2
size~dominated~T2      → size#T2~dominated
size#T2                → size#T2
size#T2~dominated      → size#T2~dominated
"
        );
    }

    /// A bare `MetricView` string means the right-hand graph, so every key
    /// written before sides existed still parses to the same thing.
    #[test]
    fn test_bare_view_is_the_right_side() {
        let parsed: MetricView = "size#eager".parse().unwrap();
        assert_eq!(parsed.side(), None);
        assert_eq!(parsed.to_string(), "size#eager");
    }

    /// `@right` was the spelling `SortColumn`'s docs advertised while its key
    /// was an untyped `String`.
    #[test]
    fn test_right_alias_is_accepted_but_never_emitted() {
        let parsed: MetricView = "size~transitive@right".parse().unwrap();
        assert_eq!(parsed.side(), None);
        assert_eq!(parsed.to_string(), "size~transitive");
    }

    /// A single-graph view has no `side` at all — not `"Right"` — so JSON is
    /// byte-identical to what was written before sides existed, and a caller
    /// can just omit the field.
    #[test]
    fn test_absent_side_means_the_primary_graph() {
        let right = serde_json::to_string(&MetricView::transitive("size")).unwrap();
        let delta = serde_json::to_string(
            &MetricView::transitive("size").with_side(Some(MetricSide::Delta)),
        )
        .unwrap();

        snapshot!(
            format!("{right}\n{delta}"),
            r#"
{"Transitive":{"name":"size"}}
{"Transitive":{"name":"size","side":"Delta"}}
"#
        );

        // A payload that omits `side` round-trips to the same value the
        // constructors produce — there is only one spelling of "no side".
        let omitted: MetricView =
            serde_json::from_str(r#"{"Transitive":{"name":"size"}}"#).unwrap();
        assert_eq!(omitted, MetricView::transitive("size"));
        assert_eq!(omitted.side(), None);
    }

    /// The side is not part of *what* is measured, so it must not leak into
    /// visibility / format lookups.
    #[test]
    fn test_base_strips_the_side() {
        let delta = MetricView::tiered("size", "eager").with_side(Some(MetricSide::Delta));
        assert_eq!(delta.base(), MetricView::tiered("size", "eager"));
        assert_eq!(delta.base().to_string(), "size#eager");
    }

    #[test]
    fn test_parse_errors() {
        assert!("node-count~unknown".parse::<MetricView>().is_err());
        assert!("size#T1~unknown".parse::<MetricView>().is_err());
        assert!("size~transitive@sideways".parse::<MetricView>().is_err());
        assert!("a~b~c~d".parse::<MetricView>().is_err());
    }
}
