// Copyright (c) Meta Platforms, Inc. and affiliates.

//! String-literal escaping for generated code.
//!
//! Constant values come straight from Rust string literals, so they can contain
//! quotes, backslashes and control characters that would otherwise terminate or
//! corrupt the literal we emit. Every generator renders values through one of
//! these two functions.

/// Escape a value for a double-quoted JavaScript/TypeScript/Flow literal.
pub(crate) fn escape_js_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            // Legal inside a JS string literal, but they terminate a line for
            // anything that re-parses the file as JSON or as pre-ES2019 source.
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            c if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }

    escaped
}

/// Escape a value for a double-quoted Hack literal.
///
/// Hack double-quoted strings interpolate `$name` and `{$expr}`, so `$` has to
/// be escaped alongside the usual suspects. Single quotes would avoid that, but
/// they have no `\n` escape — a value containing a newline would then split the
/// generated constant across lines.
pub(crate) fn escape_hack_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '$' => escaped.push_str("\\$"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => escaped.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => escaped.push(c),
        }
    }

    escaped
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::escape_hack_string;
    use super::escape_js_string;

    #[test]
    fn test_escaping() {
        let cases = [
            "plain",
            "say \"hi\"",
            "it's",
            "back\\slash",
            "$notAVariable",
            "{$alsoNotAVariable}",
            "line1\nline2",
            "tab\there",
            "bell\u{7}",
            "sep\u{2028}here",
        ];

        let rows = cases
            .iter()
            .map(|case| {
                [
                    format!("{case:?}"),
                    format!("\"{}\"", escape_js_string(case)),
                    format!("\"{}\"", escape_hack_string(case)),
                ]
            })
            .collect::<Vec<_>>();

        snapshot!(
            format_table(["rust", "js", "hack"], &rows),
            r#"
rust                  | js                    | hack
----------------------+-----------------------+-----------------------
"plain"               | "plain"               | "plain"
"say \\"hi\\""          | "say \\"hi\\""          | "say \\"hi\\""
"it's"                | "it's"                | "it's"
"back\\\\slash"         | "back\\\\slash"         | "back\\\\slash"
"$notAVariable"       | "$notAVariable"       | "\\$notAVariable"
"{$alsoNotAVariable}" | "{$alsoNotAVariable}" | "{\\$alsoNotAVariable}"
"line1\
line2"        | "line1\
line2"        | "line1\
line2"
"tab\\there"           | "tab\\there"           | "tab\\there"
"bell\\u{7}"           | "bell\\u0007"          | "bell\\u{7}"
"sep\\u{2028}here"     | "sep\\u2028here"       | "sep\u{2028}here"
"#
        );
    }

    /// Render `headers` + `rows` as a fixed-width ASCII table.
    fn format_table(headers: [&str; 3], rows: &[[String; 3]]) -> String {
        let widths: Vec<usize> = (0..3)
            .map(|col| {
                rows.iter()
                    .map(|row| row[col].chars().count())
                    .chain(std::iter::once(headers[col].len()))
                    .max()
                    .unwrap_or(0)
            })
            .collect();

        let render = |cells: [&str; 3]| {
            (0..3)
                .map(|col| format!("{:width$}", cells[col], width = widths[col]))
                .collect::<Vec<_>>()
                .join(" | ")
                .trim_end()
                .to_string()
        };

        let separator = widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("-+-");

        let mut lines = vec![render(headers), separator];
        lines.extend(rows.iter().map(|row| render([&row[0], &row[1], &row[2]])));
        lines.join("\n")
    }
}
