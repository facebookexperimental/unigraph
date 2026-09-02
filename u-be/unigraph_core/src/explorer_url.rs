// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Building explorer links.
//!
//! The counterpart to [`ExplorerUrlParams`]: that type says *what* the explorer
//! understands, this one turns it back into a path a browser can follow.
//!
//! ```text
//!   /{right}                     single graph
//!   /{left}/{right}              delta view, left first (as they sit on screen)
//!   /{right}?roots=%5B%22a%22%5D  with overrides
//!   /{right}#{node}              opened on one node
//! ```
//!
//! No scheme or host — the explorer is mounted at the root of whatever app is
//! serving it, so callers concatenate their own origin if they need an absolute
//! URL.
//!
//! ## Encoding
//!
//! Query values use the `application/x-www-form-urlencoded` rules rather than
//! plain percent-encoding: unreserved plus `*`, space as `+`. That is what
//! `URLSearchParams.toString()` emits in the browser, and the frontend writes
//! these same params through `URLSearchParams` — matching it means a link built
//! here is byte-identical to one the UI would produce, so they compare equal in
//! tests, caches, and bug reports.
//!
//! Path segments keep RFC 3986 unreserved characters instead, which is what
//! leaves `my-timeline~1` readable rather than `my-timeline%7E1`.

use anyhow::Context;
use anyhow::Result;
use percent_encoding::AsciiSet;
use percent_encoding::NON_ALPHANUMERIC;
use percent_encoding::utf8_percent_encode;
use serde::Serialize;

use crate::graph_handle::GraphHandle;
use crate::types::NodeName;
use crate::types::explorer_url_params::ExplorerUrlParams;

/// A complete explorer link: handles in the path, overrides in the query.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplorerUrl {
    /// The primary ("after") graph. Always present — a single-graph link is one
    /// of these with no `left`.
    pub right: GraphHandle,
    /// The comparison ("before") graph. `Some` turns this into a delta view.
    pub left: Option<GraphHandle>,
    pub params: ExplorerUrlParams,
    /// A node to open on, rendered as the fragment.
    ///
    /// In the fragment rather than the query because it never needs to reach
    /// the server: it cannot change which graph loads, only where to look once
    /// it has. That also keeps it out of [`ExplorerUrlParams`], which is the
    /// search-param surface.
    pub node: Option<NodeName>,
}

impl ExplorerUrl {
    /// A link to one graph.
    pub fn single(right: GraphHandle) -> Self {
        Self {
            right,
            left: None,
            params: ExplorerUrlParams::default(),
            node: None,
        }
    }

    /// A link to a delta view. `left` is the "before" side.
    pub fn compare(left: GraphHandle, right: GraphHandle) -> Self {
        Self {
            right,
            left: Some(left),
            params: ExplorerUrlParams::default(),
            node: None,
        }
    }

    /// Attach query params.
    pub fn with_params(mut self, params: ExplorerUrlParams) -> Self {
        self.params = params;
        self
    }

    /// Open the link on one node.
    pub fn with_node(mut self, node: impl Into<NodeName>) -> Self {
        self.node = Some(node.into());
        self
    }

    /// Render as a root-relative URL.
    ///
    /// Fallible only because the JSON-valued params go through `serde_json`.
    /// In practice they cannot fail — `GraphQueryConfig::cache_key` relies on
    /// the same thing — but a traversal config carrying a non-finite float
    /// would, and silently emitting a broken link is worse than saying so.
    pub fn to_url(&self) -> Result<String> {
        let path = self.path();
        let query = self.query()?;
        let fragment = match &self.node {
            Some(node) => format!("#{}", encode_segment(node)),
            None => String::new(),
        };
        if query.is_empty() {
            return Ok(format!("{path}{fragment}"));
        }
        Ok(format!("{path}?{query}{fragment}"))
    }
}

// -- Path ---------------------------------------------------------------------

impl ExplorerUrl {
    fn path(&self) -> String {
        let right = encode_segment(&self.right.to_string());
        match &self.left {
            Some(left) => format!("/{}/{right}", encode_segment(&left.to_string())),
            None => format!("/{right}"),
        }
    }
}

// -- Query --------------------------------------------------------------------

impl ExplorerUrl {
    /// Emit params in struct-declaration order, skipping absent ones.
    ///
    /// A fixed order (rather than, say, insertion order) is what makes two links
    /// describing the same view compare equal as strings.
    fn query(&self) -> Result<String> {
        let p = &self.params;
        let mut q = QueryBuilder::default();

        q.json("roots", p.roots.as_ref())?;
        q.json("roots_left", p.roots_left.as_ref())?;
        q.json("roots_right", p.roots_right.as_ref())?;
        q.json("traversal", p.traversal.as_ref())?;
        q.json("traversal_left", p.traversal_left.as_ref())?;
        q.json("traversal_right", p.traversal_right.as_ref())?;
        q.raw("graph_settings", p.graph_settings.as_deref());
        q.raw("gqc_delta_left", p.gqc_delta_left.as_deref());
        q.raw("gqc_delta_right", p.gqc_delta_right.as_deref());

        Ok(q.finish())
    }
}

#[derive(Default)]
struct QueryBuilder {
    out: String,
}

impl QueryBuilder {
    /// A param whose value is JSON — the hand-writable overrides.
    fn json<T: Serialize>(&mut self, key: &str, value: Option<&T>) -> Result<()> {
        let Some(value) = value else { return Ok(()) };
        let json = serde_json::to_string(value)
            .with_context(|| format!("failed to serialize URL param '{key}'"))?;
        self.raw(key, Some(&json));
        Ok(())
    }

    /// A param already encoded as a string (zstd+base64, or a delta blob).
    fn raw(&mut self, key: &str, value: Option<&str>) {
        let Some(value) = value else { return };
        if value.is_empty() {
            return;
        }
        if !self.out.is_empty() {
            self.out.push('&');
        }
        self.out.push_str(key);
        self.out.push('=');
        self.out.push_str(&encode_query_value(value));
    }

    fn finish(self) -> String {
        self.out
    }
}

// -- Encoding -----------------------------------------------------------------

/// RFC 3986 unreserved: everything else in a path segment gets escaped. Keeping
/// `~` literal is what makes `my-timeline~1` readable in a link.
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// `application/x-www-form-urlencoded`, matching `URLSearchParams.toString()`.
/// Space is handled separately because that serializer writes it as `+`.
const FORM_URLENCODED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'*')
    .remove(b'-')
    .remove(b'.')
    .remove(b'_');

fn encode_segment(s: &str) -> String {
    utf8_percent_encode(s, PATH_SEGMENT).to_string()
}

fn encode_query_value(s: &str) -> String {
    // `+` first: encoding the space as `+` afterwards would be ambiguous with a
    // literal `+` in the input, so that one has to become `%2B` first.
    utf8_percent_encode(s, FORM_URLENCODED)
        .to_string()
        .replace("%20", "+")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use k9::snapshot;

    use super::*;
    use crate::config_query::TraversalOverride;
    use crate::traversal::Decision;
    use crate::traversal::TraversalConfig;

    /// One table over every shape a link can take, so the encoding rules and the
    /// param ordering are both visible at a glance.
    #[test]
    fn urls_for_every_shape() {
        let cases: Vec<(&str, ExplorerUrl)> = vec![
            (
                "single, bare",
                ExplorerUrl::single(handle("my-timeline-meow~1")),
            ),
            (
                "compare",
                ExplorerUrl::compare(handle("my-timeline-meow~1"), handle("my-timeline-meow~2")),
            ),
            ("bare timeline (latest)", ExplorerUrl::single(handle("www"))),
            (
                "gqc key handle",
                ExplorerUrl::single(handle("gqc_1a2b3c4d5e6f7890")),
            ),
            (
                "shared roots",
                ExplorerUrl::single(handle("www~1")).with_params(params(|p| {
                    p.roots = roots(&["app", "core"]);
                })),
            ),
            (
                "shared roots on a compare",
                ExplorerUrl::compare(handle("www~1"), handle("www~2")).with_params(params(|p| {
                    p.roots = roots(&["app"]);
                })),
            ),
            (
                "shared roots + left override",
                ExplorerUrl::compare(handle("www~1"), handle("www~2")).with_params(params(|p| {
                    p.roots = roots(&["app"]);
                    p.roots_left = roots(&["ui"]);
                })),
            ),
            (
                "explicitly empty roots",
                ExplorerUrl::single(handle("www~1")).with_params(params(|p| {
                    p.roots = Some(BTreeSet::new());
                })),
            ),
            (
                "traversal by key",
                ExplorerUrl::single(handle("www~1")).with_params(params(|p| {
                    p.traversal = Some(TraversalOverride::Key("tvc_00ff".parse().unwrap()));
                })),
            ),
            (
                "traversal inline",
                ExplorerUrl::single(handle("www~1")).with_params(params(|p| {
                    p.traversal = Some(TraversalOverride::Inline(tvc_forcing("a")));
                })),
            ),
            (
                "opaque params pass through",
                ExplorerUrl::single(handle("www~1")).with_params(params(|p| {
                    p.graph_settings = Some("H4sIAAAA-_w".to_owned());
                    p.gqc_delta_right = Some("KLUv_QBY".to_owned());
                })),
            ),
            (
                "opened on a node",
                ExplorerUrl::single(handle("www~1")).with_node("pkg/Mod.js"),
            ),
            (
                "node fragment after query",
                ExplorerUrl::single(handle("www~1"))
                    .with_params(params(|p| p.roots = roots(&["app"])))
                    .with_node("pkg/Mod.js"),
            ),
            (
                "every param at once",
                ExplorerUrl::compare(handle("www~1"), handle("www~2")).with_params(params(|p| {
                    p.roots = roots(&["a"]);
                    p.roots_left = roots(&["b"]);
                    p.roots_right = roots(&["c"]);
                    p.traversal = Some(TraversalOverride::Key("tvc_1".parse().unwrap()));
                    p.traversal_left = Some(TraversalOverride::Key("tvc_2".parse().unwrap()));
                    p.traversal_right = Some(TraversalOverride::Key("tvc_3".parse().unwrap()));
                    p.graph_settings = Some("gs".to_owned());
                    p.gqc_delta_left = Some("dl".to_owned());
                    p.gqc_delta_right = Some("dr".to_owned());
                })),
            ),
        ];

        snapshot!(
            format_cases(&cases),
            "
single, bare                    /my-timeline-meow~1
compare                         /my-timeline-meow~1/my-timeline-meow~2
bare timeline (latest)          /www
gqc key handle                  /gqc_1a2b3c4d5e6f7890
shared roots                    /www~1?roots=%5B%22app%22%2C%22core%22%5D
shared roots on a compare       /www~1/www~2?roots=%5B%22app%22%5D
shared roots + left override    /www~1/www~2?roots=%5B%22app%22%5D&roots_left=%5B%22ui%22%5D
explicitly empty roots          /www~1?roots=%5B%5D
traversal by key                /www~1?traversal=%7B%22Key%22%3A%22tvc_00ff%22%7D
traversal inline                /www~1?traversal=%7B%22Inline%22%3A%7B%22force_nodes%22%3A%7B%22a%22%3A%7B%22include%22%3Atrue%2C%22message_id%22%3Anull%7D%7D%7D%7D
opaque params pass through      /www~1?graph_settings=H4sIAAAA-_w&gqc_delta_right=KLUv_QBY
opened on a node                /www~1#pkg%2FMod.js
node fragment after query       /www~1?roots=%5B%22app%22%5D#pkg%2FMod.js
every param at once             /www~1/www~2?roots=%5B%22a%22%5D&roots_left=%5B%22b%22%5D&roots_right=%5B%22c%22%5D&traversal=%7B%22Key%22%3A%22tvc_1%22%7D&traversal_left=%7B%22Key%22%3A%22tvc_2%22%7D&traversal_right=%7B%22Key%22%3A%22tvc_3%22%7D&graph_settings=gs&gqc_delta_left=dl&gqc_delta_right=dr
"
        );
    }

    /// Node names are user data — they can carry anything, including the
    /// characters that would otherwise terminate a param or a path segment.
    #[test]
    fn hostile_node_names_are_escaped() {
        let cases: Vec<(&str, ExplorerUrl)> = vec![
            (
                "ampersand + equals",
                ExplorerUrl::single(handle("www~1"))
                    .with_params(params(|p| p.roots = roots(&["a&b=c"]))),
            ),
            (
                "space and plus",
                ExplorerUrl::single(handle("www~1"))
                    .with_params(params(|p| p.roots = roots(&["a b+c"]))),
            ),
            (
                "hash and percent",
                ExplorerUrl::single(handle("www~1"))
                    .with_params(params(|p| p.roots = roots(&["a#b%c"]))),
            ),
            (
                "slash in a node name",
                ExplorerUrl::single(handle("www~1"))
                    .with_params(params(|p| p.roots = roots(&["pkg/mod.rs"]))),
            ),
            (
                "non-ascii",
                ExplorerUrl::single(handle("www~1"))
                    .with_params(params(|p| p.roots = roots(&["meow🐈"]))),
            ),
        ];

        snapshot!(
            format_cases(&cases),
            "
ampersand + equals              /www~1?roots=%5B%22a%26b%3Dc%22%5D
space and plus                  /www~1?roots=%5B%22a+b%2Bc%22%5D
hash and percent                /www~1?roots=%5B%22a%23b%25c%22%5D
slash in a node name            /www~1?roots=%5B%22pkg%2Fmod.rs%22%5D
non-ascii                       /www~1?roots=%5B%22meow%F0%9F%90%88%22%5D
"
        );
    }

    /// The whole point of matching `URLSearchParams`: a link built here must
    /// come back out of a standards-compliant parser unchanged.
    #[test]
    fn query_values_survive_a_form_urlencoded_round_trip() {
        let names = ["a&b=c", "a b+c", "a#b%c", "pkg/mod.rs", "meow🐈", "plain"];
        let url = ExplorerUrl::single(handle("www~1"))
            .with_params(params(|p| p.roots = roots(&names)))
            .to_url()
            .unwrap();

        let query = url.split_once('?').expect("params were set").1;
        let (key, value) = query.split_once('=').expect("one param");
        assert_eq!(key, "roots");

        let decoded = form_urldecode(value);
        let parsed: BTreeSet<String> = serde_json::from_str(&decoded).expect("valid JSON");
        assert_eq!(
            parsed,
            names
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<BTreeSet<_>>(),
            "every node name must survive encode -> decode"
        );
    }

    /// Absent is not the same as present-and-empty: an empty opaque string would
    /// produce a dangling `key=`, which round-trips as `Some("")` rather than
    /// `None` and would then be decoded as a real (broken) value.
    #[test]
    fn empty_opaque_params_are_omitted() {
        let url = ExplorerUrl::single(handle("www~1"))
            .with_params(params(|p| {
                p.graph_settings = Some(String::new());
                p.gqc_delta_left = Some(String::new());
            }))
            .to_url()
            .unwrap();
        assert_eq!(url, "/www~1", "empty values must not emit a bare `key=`");
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn handle(s: &str) -> GraphHandle {
        s.parse().expect("test handle must parse")
    }

    fn roots(names: &[&str]) -> Option<BTreeSet<String>> {
        Some(names.iter().map(|s| (*s).to_owned()).collect())
    }

    fn params(build: impl FnOnce(&mut ExplorerUrlParams)) -> ExplorerUrlParams {
        let mut p = ExplorerUrlParams::default();
        build(&mut p);
        p
    }

    fn tvc_forcing(node: &str) -> TraversalConfig {
        TraversalConfig {
            force_nodes: Some(BTreeMap::from([(node.to_owned(), Decision::include())])),
            ..Default::default()
        }
    }

    fn format_cases(cases: &[(&str, ExplorerUrl)]) -> String {
        cases
            .iter()
            .map(|(label, url)| format!("{label:<32}{}", url.to_url().expect("URL must render")))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Minimal `application/x-www-form-urlencoded` decoder, so the round-trip
    /// test checks against the spec rather than against our own encoder.
    fn form_urldecode(s: &str) -> String {
        let bytes = s.replace('+', " ").into_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("ascii hex");
                out.push(u8::from_str_radix(hex, 16).expect("valid percent escape"));
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).expect("decoded bytes must be UTF-8")
    }
}
