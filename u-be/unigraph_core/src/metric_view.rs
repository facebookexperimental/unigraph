// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::fmt;
use std::str::FromStr;

use anyhow::bail;

const SEPARATOR: char = '~';
const TIER_SEPARATOR: char = '#';
const NODE_COUNT: &str = "node-count";
const PARENTS_COUNT: &str = "parents-count";
const TIER_INDEX: &str = "tier";
const TRANSITIVE: &str = "transitive";
const DOMINATED: &str = "dominated";

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
}
