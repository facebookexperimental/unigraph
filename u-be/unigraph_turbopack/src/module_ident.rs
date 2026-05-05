// Copyright (c) Meta Platforms, Inc. and affiliates.

/// Parse turbopack module identifiers.
///
/// Ident format: `path {templateArgs} [layer] (moduleType) <fragment>`
///
/// Examples:
///   `[project]/src/app/page.tsx [app-rsc] (ecmascript)`
///   `[project]/node_modules/react/index.js [app-client] (ecmascript)`
///   `[project]/src/utils.ts [app-rsc] (ecmascript) <exports>`

#[derive(Debug, Clone)]
pub struct ParsedIdent {
    pub path: String,
    pub layer: Option<String>,
    #[expect(
        dead_code,
        reason = "parsed from ident but not yet used in node ID construction"
    )]
    pub module_type: Option<String>,
    pub fragment: Option<String>,
}

/// Parse a turbopack module ident string into its components.
///
/// Works right-to-left, peeling off `<fragment>`, `(moduleType)`, `[layer]`,
/// `{templateArgs}` suffixes, leaving the path as the remainder.
pub fn parse_ident(ident: &str) -> ParsedIdent {
    let mut remaining = ident.trim();

    let fragment = peel_suffix(remaining, '<', '>');
    if let Some((rest, _)) = &fragment {
        remaining = rest.trim();
    }

    let module_type = peel_suffix(remaining, '(', ')');
    if let Some((rest, _)) = &module_type {
        remaining = rest.trim();
    }

    let layer = peel_suffix(remaining, '[', ']');
    if let Some((rest, _)) = &layer {
        remaining = rest.trim();
    }

    // Template args — peel but discard.
    let template_args = peel_suffix(remaining, '{', '}');
    if let Some((rest, _)) = &template_args {
        remaining = rest.trim();
    }

    ParsedIdent {
        path: remaining.to_string(),
        layer: layer.map(|(_, v)| v),
        module_type: module_type.map(|(_, v)| v),
        fragment: fragment.map(|(_, v)| v),
    }
}

/// Build a node ID from a parsed ident.
///
/// Always includes layer (nodes in different layers are separate).
/// When `collapse_fragments` is true, fragment suffixes are stripped,
/// causing multiple fragments of the same module to merge into one node.
pub fn node_id(parsed: &ParsedIdent, collapse_fragments: bool) -> String {
    let mut id = parsed.path.clone();
    if let Some(layer) = &parsed.layer {
        id = format!("{id} [{layer}]");
    }
    if !collapse_fragments && let Some(fragment) = &parsed.fragment {
        id = format!("{id} <{fragment}>");
    }
    id
}

/// If `s` ends with `close`, find the matching `open` and return
/// `(everything_before_open, content_between)`.
fn peel_suffix(s: &str, open: char, close: char) -> Option<(&str, String)> {
    let s = s.trim_end();
    if !s.ends_with(close) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for i in (0..bytes.len()).rev() {
        let ch = bytes[i] as char;
        if ch == close {
            depth += 1;
        } else if ch == open {
            depth -= 1;
            if depth == 0 {
                let content = &s[i + open.len_utf8()..s.len() - close.len_utf8()];
                let before = &s[..i];
                return Some((before, content.to_string()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_ident() {
        let p = parse_ident("[project]/src/app/page.tsx [app-rsc] (ecmascript)");
        assert_eq!(p.path, "[project]/src/app/page.tsx");
        assert_eq!(p.layer.as_deref(), Some("app-rsc"));
        assert_eq!(p.module_type.as_deref(), Some("ecmascript"));
        assert!(p.fragment.is_none());
    }

    #[test]
    fn test_ident_with_fragment() {
        let p = parse_ident("[project]/src/utils.ts [app-rsc] (ecmascript) <exports>");
        assert_eq!(p.path, "[project]/src/utils.ts");
        assert_eq!(p.layer.as_deref(), Some("app-rsc"));
        assert_eq!(p.fragment.as_deref(), Some("exports"));
    }

    #[test]
    fn test_ident_no_layer() {
        let p = parse_ident("[project]/src/global.css (css)");
        assert_eq!(p.path, "[project]/src/global.css");
        assert!(p.layer.is_none());
        assert_eq!(p.module_type.as_deref(), Some("css"));
    }

    #[test]
    fn test_node_id_with_layer() {
        let p = parse_ident("[project]/src/app/page.tsx [app-rsc] (ecmascript)");
        assert_eq!(node_id(&p, false), "[project]/src/app/page.tsx [app-rsc]");
    }

    #[test]
    fn test_node_id_fragment_expanded() {
        let p = parse_ident("[project]/src/utils.ts [app-rsc] (ecmascript) <exports>");
        assert_eq!(
            node_id(&p, false),
            "[project]/src/utils.ts [app-rsc] <exports>"
        );
    }

    #[test]
    fn test_node_id_fragment_collapsed() {
        let p = parse_ident("[project]/src/utils.ts [app-rsc] (ecmascript) <exports>");
        assert_eq!(node_id(&p, true), "[project]/src/utils.ts [app-rsc]");
    }
}
