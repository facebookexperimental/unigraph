// Copyright (c) Meta Platforms, Inc. and affiliates.

//! The explorer's URL surface — every search param it understands, as one type.
//!
//! The path carries the handles (`/{right}` for a single graph, `/{left}/{right}`
//! for a delta view). Everything else rides in the query string, and this module
//! is the only place that knows those key names.
//!
//! ## Two levels of specificity
//!
//! Each per-side setting has three keys: a bare one and a `_left`/`_right` pair.
//! The bare key applies to both sides; a side-specific key overrides it for that
//! side alone.
//!
//! ```text
//!   ?roots=["a"]                  left: a       right: a
//!   ?roots_left=["b"]             left: b       right: (default)
//!   ?roots=["a"]&roots_left=["b"] left: b       right: a
//! ```
//!
//! The point is that adding a second handle to a URL you are already looking at
//! carries your overrides across to the new side, instead of silently leaving it
//! on defaults — and you can then narrow one side without restating the other.
//!
//! `graph_settings` has no per-side variants: the explorer holds a single
//! settings instance, defaulting to the right graph.

use std::collections::BTreeSet;

use crate::config_query::TraversalOverride;
use crate::types::NodeName;

/// Every explorer URL search param, flat and all-optional.
///
/// Flat rather than nested so it maps one-to-one onto query-string keys: each
/// field name *is* the param name, which is what lets consumers build and read
/// URLs off the generated type instead of scattered string literals.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen
)]
pub struct ExplorerUrlParams {
    /// Entry-point override for both sides.
    pub roots: Option<BTreeSet<NodeName>>,
    /// Entry-point override for the left ("before") graph only.
    pub roots_left: Option<BTreeSet<NodeName>>,
    /// Entry-point override for the right ("after") graph only.
    pub roots_right: Option<BTreeSet<NodeName>>,

    /// Traversal override for both sides.
    pub traversal: Option<TraversalOverride>,
    /// Traversal override for the left ("before") graph only.
    pub traversal_left: Option<TraversalOverride>,
    /// Traversal override for the right ("after") graph only.
    pub traversal_right: Option<TraversalOverride>,

    /// Metric/column view settings, zstd+base64 encoded. Single instance rather
    /// than a per-side pair: the explorer holds one settings object, defaulting
    /// to the right graph.
    ///
    /// Opaque for the same reason as the deltas below — it is not JSON, so it
    /// cannot be hand-written, and decoding it needs the WASM codec.
    pub graph_settings: Option<String>,

    /// Opaque `GraphQueryConfig` delta the traversal editor writes, per side.
    ///
    /// Kept opaque here on purpose: it is a compact encoding of a UI edit, not
    /// something anyone hand-writes. It stays separate from `traversal_*` because
    /// a full inline `TraversalConfig` can exceed 100 KB while its delta is a few
    /// hundred bytes, so the editor cannot round-trip through `traversal_*`.
    pub gqc_delta_left: Option<String>,
    /// See [`gqc_delta_left`](Self::gqc_delta_left).
    pub gqc_delta_right: Option<String>,
}

/// One side's overrides after the bare/`_left`/`_right` fallback has been applied.
///
/// Mirrors the override fields of [`GraphQueryConfig`](crate::config_query::GraphQueryConfig)
/// so a caller can drop these straight onto a handle.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SideOverrides {
    pub roots: Option<BTreeSet<NodeName>>,
    pub traversal: Option<TraversalOverride>,
}

/// Both sides' overrides. `left` is meaningful only in delta view; a single-graph
/// caller reads `right` and ignores `left`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedOverrides {
    pub left: SideOverrides,
    pub right: SideOverrides,
}

impl ExplorerUrlParams {
    /// Apply the bare/per-side fallback and hand back what each side should use.
    pub fn resolve(&self) -> ResolvedOverrides {
        ResolvedOverrides {
            left: SideOverrides {
                roots: pick(self.roots_left.as_ref(), self.roots.as_ref()),
                traversal: pick(self.traversal_left.as_ref(), self.traversal.as_ref()),
            },
            right: SideOverrides {
                roots: pick(self.roots_right.as_ref(), self.roots.as_ref()),
                traversal: pick(self.traversal_right.as_ref(), self.traversal.as_ref()),
            },
        }
    }
}

/// The whole fallback rule: a side-specific value wins, otherwise the shared one.
fn pick<T: Clone>(side: Option<&T>, shared: Option<&T>) -> Option<T> {
    side.or(shared).cloned()
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;
    use crate::traversal::TraversalConfig;

    /// Every combination of "shared set?" x "left set?" x "right set?", so the
    /// precedence rule is visible as a table rather than spread across cases.
    #[test]
    fn bare_key_falls_through_to_both_sides() {
        let cases = [
            (None, None, None),
            (Some("s"), None, None),
            (None, Some("l"), None),
            (None, None, Some("r")),
            (Some("s"), Some("l"), None),
            (Some("s"), None, Some("r")),
            (None, Some("l"), Some("r")),
            (Some("s"), Some("l"), Some("r")),
        ];

        let rows: Vec<String> = cases
            .iter()
            .map(|(shared, left, right)| {
                let params = ExplorerUrlParams {
                    roots: roots(*shared),
                    roots_left: roots(*left),
                    roots_right: roots(*right),
                    traversal: None,
                    traversal_left: None,
                    traversal_right: None,
                    graph_settings: None,
                    gqc_delta_left: None,
                    gqc_delta_right: None,
                };
                let out = params.resolve();
                format!(
                    "{:<7} {:<7} {:<7} | {:<7} {}",
                    show(*shared),
                    show(*left),
                    show(*right),
                    show_roots(&out.left.roots),
                    show_roots(&out.right.roots),
                )
            })
            .collect();

        let table = format!(
            "{:<7} {:<7} {:<7} | {:<7} {}\n{}",
            "roots",
            "_left",
            "_right",
            "L",
            "R",
            rows.join("\n")
        );

        snapshot!(
            table,
            "
roots   _left   _right  | L       R
-       -       -       | -       -
s       -       -       | s       s
-       l       -       | l       -
-       -       r       | -       r
s       l       -       | l       s
s       -       r       | s       r
-       l       r       | l       r
s       l       r       | l       r
"
        );
    }

    /// `traversal` follows the identical rule — the fallback is per-setting, not
    /// special-cased per type.
    #[test]
    fn traversal_uses_the_same_fallback() {
        let params = ExplorerUrlParams {
            roots: None,
            roots_left: None,
            roots_right: None,
            traversal: Some(TraversalOverride::Key("tvc_shared".parse().unwrap())),
            traversal_left: Some(TraversalOverride::Inline(TraversalConfig::default())),
            traversal_right: None,
            graph_settings: None,
            gqc_delta_left: None,
            gqc_delta_right: None,
        };

        let out = params.resolve();
        assert!(
            matches!(out.left.traversal, Some(TraversalOverride::Inline(_))),
            "left must take its own override"
        );
        assert!(
            matches!(out.right.traversal, Some(TraversalOverride::Key(_))),
            "right must fall through to the shared key"
        );
    }

    /// An empty root set is a real value ("no entry points"), not absence — it
    /// must override the shared key rather than fall through to it.
    #[test]
    fn empty_roots_override_rather_than_fall_through() {
        let params = ExplorerUrlParams {
            roots: roots(Some("s")),
            roots_left: Some(BTreeSet::new()),
            roots_right: None,
            traversal: None,
            traversal_left: None,
            traversal_right: None,
            graph_settings: None,
            gqc_delta_left: None,
            gqc_delta_right: None,
        };

        let out = params.resolve();
        assert_eq!(
            out.left.roots,
            Some(BTreeSet::new()),
            "an explicit empty set must not fall through to the shared roots"
        );
        assert_eq!(out.right.roots, roots(Some("s")));
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn roots(name: Option<&str>) -> Option<BTreeSet<NodeName>> {
        name.map(|n| BTreeSet::from([n.to_owned()]))
    }

    fn show(name: Option<&str>) -> &str {
        name.unwrap_or("-")
    }

    fn show_roots(set: &Option<BTreeSet<NodeName>>) -> String {
        match set {
            None => "-".to_owned(),
            Some(s) if s.is_empty() => "(empty)".to_owned(),
            Some(s) => s.iter().cloned().collect::<Vec<_>>().join(","),
        }
    }
}
