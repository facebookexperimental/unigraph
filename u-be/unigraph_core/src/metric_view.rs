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
/// Never emitted — `Right` is the bare form. Accepted only because
/// `SortColumn`'s docs advertised it before these keys were typed, so it may
/// exist in hand-written graph settings. See [`MetricColumn::from_str`].
const RIGHT_ALIAS: &str = "right";

/// A user-facing metric specification.
///
/// Describes which metric to compute for a node. Not the raw data itself,
/// but the *view* — plain value, transitive sum, dominated sum, tiered, or
/// a structural count like parent count or transitive node count.
///
/// ## String format
///
/// `MetricView` implements `Display` and `FromStr`. The `~` separator
/// separates the metric name from the view variant, while `#` introduces
/// a tier name:
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
    Metric { name: String },
    /// Transitive metric sum (DFS over forward edges).
    Transitive { name: String },
    /// Dominated metric sum (DFS over dominator tree).
    Dominated { name: String },
    /// Tiered transitive metric (cumulative at a specific tier).
    Tiered { name: String, tier_name: String },
    /// Tiered dominated metric (dominated sum at a specific tier).
    TieredDominated { name: String, tier_name: String },
    /// Number of configured parents (incoming edges).
    ParentsCount {},
    /// Transitive dependency count (forward DFS).
    CountTransitive {},
    /// Dominated dependency count (dominator tree DFS).
    CountDominated {},
    /// Tier index of the node (0-based). Only available when tiers are configured.
    TierIndex {},
}

impl MetricView {
    /// The source metric name, if this view is derived from a named metric.
    /// Returns `None` for structural counts (ParentsCount, CountTransitive, CountDominated).
    pub fn metric_name(&self) -> Option<&str> {
        match self {
            MetricView::Metric { name }
            | MetricView::Transitive { name }
            | MetricView::Dominated { name }
            | MetricView::Tiered { name, .. }
            | MetricView::TieredDominated { name, .. } => Some(name),
            MetricView::ParentsCount {}
            | MetricView::CountTransitive {}
            | MetricView::CountDominated {}
            | MetricView::TierIndex {} => None,
        }
    }

    pub fn is_dominated(&self) -> bool {
        matches!(
            self,
            MetricView::Dominated { .. }
                | MetricView::TieredDominated { .. }
                | MetricView::CountDominated {}
        )
    }
}

impl fmt::Display for MetricView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricView::Metric { name } => write!(f, "{name}"),
            MetricView::Transitive { name } => write!(f, "{name}{SEPARATOR}{TRANSITIVE}"),
            MetricView::Dominated { name } => write!(f, "{name}{SEPARATOR}{DOMINATED}"),
            MetricView::Tiered { name, tier_name } => {
                write!(f, "{name}{TIER_SEPARATOR}{tier_name}")
            }
            MetricView::TieredDominated { name, tier_name } => {
                write!(f, "{name}{TIER_SEPARATOR}{tier_name}{SEPARATOR}{DOMINATED}")
            }
            MetricView::ParentsCount {} => write!(f, "{PARENTS_COUNT}"),
            MetricView::CountTransitive {} => write!(f, "{NODE_COUNT}{SEPARATOR}{TRANSITIVE}"),
            MetricView::CountDominated {} => write!(f, "{NODE_COUNT}{SEPARATOR}{DOMINATED}"),
            MetricView::TierIndex {} => write!(f, "{TIER_INDEX}"),
        }
    }
}

impl FromStr for MetricView {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        // Split on '#' first to separate metric name from tier info.
        if let Some((metric_part, tier_part)) = s.split_once(TIER_SEPARATOR) {
            return parse_tiered(metric_part, tier_part);
        }
        parse_non_tiered(s)
    }
}

// ── Table column keys ───────────────────────────────────────────

/// Which graph a metric column reads from, when a table is comparing two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetricSide {
    /// The "before" graph.
    Left,
    /// The "after" graph. The default, so a bare `size~transitive` means this.
    #[default]
    Right,
    /// Right minus left. Not always a plain subtraction — tiered and
    /// node-count deltas exclude nodes that didn't change.
    Delta,
}

/// The identity of a column in a metrics table: *what* to measure
/// ([`MetricView`]) and *which graph* to read it from ([`MetricSide`]).
///
/// The two are deliberately separate types. A [`MetricView`] is a property of
/// one graph — `ArrayGraph::metric_value`, `available_metric_views`, and the
/// `metrics_visibility` map all operate on a single graph, where a side would
/// be meaningless (and, folded into the variants, would be *representable*).
/// A side only exists once you are naming a column in a table that may be
/// showing two graphs at once.
///
/// Used as the sort key in [`GraphTableSort`], as the column list of the
/// ExploreDelta RPC, and as the keys of that RPC's metrics map. A bare view
/// means the right-hand graph, so every single-graph key is also a valid
/// `MetricColumn`:
///
/// ```text
/// size~transitive             → Right   (bare == right graph)
/// size~transitive@left        → Left
/// size~transitive@delta       → Delta
/// size#lazy@delta             → Delta of a tiered view
/// node-count~transitive@delta → Delta of the transitive node count
/// ```
///
/// Serializes as that string rather than as a struct: these keys are persisted
/// inside `GraphSettings` on stored graphs, and the frontend indexes them as
/// plain strings.
///
/// [`GraphTableSort`]: crate::graph_settings::GraphTableSort
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricColumn {
    pub view: MetricView,
    pub side: MetricSide,
}

impl MetricColumn {
    pub fn new(view: MetricView, side: MetricSide) -> Self {
        Self { view, side }
    }

    /// A column reading the right-hand graph — the only graph, in single-graph
    /// mode.
    pub fn right(view: MetricView) -> Self {
        Self::new(view, MetricSide::Right)
    }

    pub fn is_delta(&self) -> bool {
        self.side == MetricSide::Delta
    }
}

impl From<MetricView> for MetricColumn {
    fn from(view: MetricView) -> Self {
        Self::right(view)
    }
}

impl fmt::Display for MetricColumn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.side {
            MetricSide::Right => write!(f, "{}", self.view),
            MetricSide::Left => write!(f, "{}{SIDE_SEPARATOR}{LEFT}", self.view),
            MetricSide::Delta => write!(f, "{}{SIDE_SEPARATOR}{DELTA}", self.view),
        }
    }
}

impl FromStr for MetricColumn {
    type Err = anyhow::Error;

    /// `@right` parses to [`MetricSide::Right`] but is never produced —
    /// `SortColumn`'s docs described that spelling while the key was still an
    /// untyped `String`, so it may sit in hand-written graph settings. Since
    /// these keys are persisted inside stored graphs, rejecting one would fail
    /// the whole `GraphSettings` load over a cosmetic sort preference.
    fn from_str(s: &str) -> anyhow::Result<Self> {
        let Some((view, side)) = s.split_once(SIDE_SEPARATOR) else {
            return Ok(Self::right(s.parse()?));
        };
        let side = match side {
            LEFT => MetricSide::Left,
            DELTA => MetricSide::Delta,
            RIGHT_ALIAS => MetricSide::Right,
            other => bail!("unknown metric side: '{other}' (expected '{LEFT}' or '{DELTA}')"),
        };
        Ok(Self::new(view.parse()?, side))
    }
}

impl serde::Serialize for MetricColumn {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for MetricColumn {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Parse a tiered metric view: `name#tier` or `name#tier~modifier`.
fn parse_tiered(name: &str, tier_part: &str) -> anyhow::Result<MetricView> {
    let name = name.to_string();
    match tier_part.split_once(SEPARATOR) {
        None => Ok(MetricView::Tiered {
            name,
            tier_name: tier_part.to_string(),
        }),
        Some((tier_name, DOMINATED)) => Ok(MetricView::TieredDominated {
            name,
            tier_name: tier_name.to_string(),
        }),
        Some((_, modifier)) => {
            bail!("unknown tiered modifier: '{modifier}' (expected 'dominated')")
        }
    }
}

/// Parse a non-tiered metric view (no `#` present).
fn parse_non_tiered(s: &str) -> anyhow::Result<MetricView> {
    let parts: Vec<&str> = s.split(SEPARATOR).collect();
    match parts.as_slice() {
        [PARENTS_COUNT] => Ok(MetricView::ParentsCount {}),
        [TIER_INDEX] => Ok(MetricView::TierIndex {}),
        [NODE_COUNT, TRANSITIVE] => Ok(MetricView::CountTransitive {}),
        [NODE_COUNT, DOMINATED] => Ok(MetricView::CountDominated {}),
        [NODE_COUNT, other] => {
            bail!("unknown node-count variant: '{other}' (expected 'transitive' or 'dominated')")
        }
        [name] => Ok(MetricView::Metric {
            name: name.to_string(),
        }),
        [name, TRANSITIVE] => Ok(MetricView::Transitive {
            name: name.to_string(),
        }),
        [name, DOMINATED] => Ok(MetricView::Dominated {
            name: name.to_string(),
        }),
        _ => bail!("invalid metric view: '{s}'"),
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;

    fn all_variants() -> Vec<MetricView> {
        vec![
            MetricView::Metric {
                name: "size".into(),
            },
            MetricView::Transitive {
                name: "size".into(),
            },
            MetricView::Dominated {
                name: "size".into(),
            },
            MetricView::Tiered {
                name: "size".into(),
                tier_name: "T1".into(),
            },
            MetricView::TieredDominated {
                name: "size".into(),
                tier_name: "T1".into(),
            },
            MetricView::ParentsCount {},
            MetricView::CountTransitive {},
            MetricView::CountDominated {},
        ]
    }

    fn format_overview(views: &[MetricView]) -> String {
        let mut out = format!("{:<26} {}\n", "display", "metric_name");
        for view in views {
            let display = view.to_string();
            let parsed: MetricView = display.parse().unwrap_or_else(|e| {
                panic!("roundtrip failed for '{display}': {e}");
            });
            assert_eq!(&parsed, view, "roundtrip mismatch for '{display}'");

            out.push_str(&format!(
                "{:<26} {}\n",
                display,
                view.metric_name().unwrap_or("-"),
            ));
        }
        out
    }

    #[test]
    fn test_all_variants() {
        snapshot!(
            format_overview(&all_variants()),
            "
display                    metric_name
size                       size
size~transitive            size
size~dominated             size
size#T1                    size
size#T1~dominated          size
parents-count              -
node-count~transitive      -
node-count~dominated       -

"
        );
    }

    #[test]
    fn test_parse_errors() {
        assert!("node-count~unknown".parse::<MetricView>().is_err());
        assert!("size#T1~unknown".parse::<MetricView>().is_err());
    }

    /// Every view × side, rendered and parsed back. One table so the whole
    /// column vocabulary — the strings that end up in `SortColumn`, in stored
    /// `GraphSettings`, and in the frontend's key comparisons — is visible in
    /// one place.
    #[test]
    fn test_twin_metric_view_roundtrip() {
        let sides = [MetricSide::Right, MetricSide::Left, MetricSide::Delta];
        let mut out = format!("{:<32} {}\n", "display", "is_delta");

        for view in all_variants().into_iter().chain([MetricView::TierIndex {}]) {
            for side in sides {
                let column = MetricColumn::new(view.clone(), side);
                let display = column.to_string();
                let parsed: MetricColumn = display
                    .parse()
                    .unwrap_or_else(|e| panic!("roundtrip failed for '{display}': {e}"));
                assert_eq!(parsed, column, "roundtrip mismatch for '{display}'");
                out.push_str(&format!("{:<32} {}\n", display, column.is_delta()));
            }
        }

        snapshot!(
            out,
            "
display                          is_delta
size                             false
size@left                        false
size@delta                       true
size~transitive                  false
size~transitive@left             false
size~transitive@delta            true
size~dominated                   false
size~dominated@left              false
size~dominated@delta             true
size#T1                          false
size#T1@left                     false
size#T1@delta                    true
size#T1~dominated                false
size#T1~dominated@left           false
size#T1~dominated@delta          true
parents-count                    false
parents-count@left               false
parents-count@delta              true
node-count~transitive            false
node-count~transitive@left       false
node-count~transitive@delta      true
node-count~dominated             false
node-count~dominated@left        false
node-count~dominated@delta       true
tier                             false
tier@left                        false
tier@delta                       true

"
        );
    }

    /// A bare `MetricView` string is a valid column key meaning "right graph",
    /// so every key written before twin mode existed still parses.
    #[test]
    fn test_bare_view_is_the_right_side() {
        let parsed: MetricColumn = "size#eager".parse().unwrap();
        assert_eq!(parsed.side, MetricSide::Right);
        assert_eq!(parsed.to_string(), "size#eager");
    }

    /// Serializes as its display string, not as a struct — stored
    /// `GraphSettings` carry these as plain JSON strings.
    #[test]
    fn test_serializes_as_a_plain_string() {
        let column = MetricColumn::new(
            MetricView::Transitive {
                name: "size".into(),
            },
            MetricSide::Delta,
        );
        let json = serde_json::to_string(&column).unwrap();
        assert_eq!(json, r#""size~transitive@delta""#);
        assert_eq!(serde_json::from_str::<MetricColumn>(&json).unwrap(), column);
    }

    /// `@right` was the spelling `SortColumn`'s docs advertised while the key
    /// was an untyped `String`. Stored graph settings may carry it, and a
    /// parse failure there would sink the entire graph load — so it is
    /// accepted, and normalizes to the bare form on the way out.
    #[test]
    fn test_right_alias_is_accepted_but_never_emitted() {
        let parsed: MetricColumn = "size~transitive@right".parse().unwrap();
        assert_eq!(parsed.side, MetricSide::Right);
        assert_eq!(parsed.to_string(), "size~transitive");
    }

    #[test]
    fn test_twin_parse_errors() {
        assert!("size~transitive@sideways".parse::<MetricColumn>().is_err());
        assert!("size~bogus@delta".parse::<MetricColumn>().is_err());
    }
}
