// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Formatting primitives for the ASCII tables the explore RPCs render for
//! agent / LLM consumption.
//!
//! Only the mechanical bits live here — cell padding, separators, and turning
//! an `f64` into the string a metric's `MetricFormat` asks for. Each RPC still
//! assembles its own header and rows, because their column sets differ.
//!
//! ```text
//! node_name | size~transitive ▼ | ∆T(size)
//! ==========+===================+=========
//! app       |           1.99 kB |  +120 B
//! ```

use std::collections::BTreeMap;

use unigraph_core::MetricView;
use unigraph_core::NameMatchMode;
use unigraph_core::NodeSelection;
use unigraph_core::graph_settings::MetricFormat;
use unigraph_core::graph_settings::MetricsConfig;
use unigraph_core::graph_settings::SortOrder;

/// Display width of a sort arrow suffix (" ▼" or " ▲"). The glyph is 3 bytes
/// but one column wide, so padding has to be computed by hand.
pub const SORT_ARROW_DISPLAY_LEN: usize = 2;

/// A [`NodeSelection`] as one human-readable line, e.g.
/// `{name~substring "comet", type=budget, has oncall, in-tag lazy}`.
///
/// Shared by both explore RPCs so a `Matching` target reads the same whether
/// you're exploring one graph or a delta.
pub fn describe_selection(selection: &NodeSelection) -> String {
    let name = selection.name_condition().map(|name| {
        let mode = match name.mode {
            NameMatchMode::Substring => "substring",
            NameMatchMode::Regex => "regex",
            NameMatchMode::Fuzzy => "fuzzy",
            NameMatchMode::Exact => "exact",
        };
        format!("name~{mode} {:?}", name.pattern)
    });

    // An absent value means "carries this property at all", which is a
    // different condition from matching the empty string.
    let properties = selection
        .properties
        .iter()
        .map(|(key, value)| match &value.value {
            Some(value) => format!("{key}={value}"),
            None => format!("has {key}"),
        });

    let edges = [
        ("in-tag", &selection.incoming_tags),
        ("out-tag", &selection.outgoing_tags),
        ("in-dyn", &selection.incoming_dynamic_type_keys),
        ("out-dyn", &selection.outgoing_dynamic_type_keys),
    ]
    .into_iter()
    .flat_map(|(label, values)| values.iter().map(move |value| format!("{label} {value}")));

    let parts: Vec<String> = name.into_iter().chain(properties).chain(edges).collect();
    if parts.is_empty() {
        return "{any node}".to_string();
    }
    format!("{{{}}}", parts.join(", "))
}

pub fn sort_arrow(order: SortOrder) -> &'static str {
    match order {
        SortOrder::Desc => " ▼",
        SortOrder::Asc => " ▲",
    }
}

/// Write `text` padded to `width`. Left-aligned for names and labels,
/// right-aligned for numbers.
pub fn write_cell(out: &mut String, text: &str, width: usize, left_align: bool) {
    write_padded(out, text, text.len(), width, left_align);
}

/// [`write_cell`] for text whose display width isn't its byte length — a sort
/// arrow is three bytes but one column.
pub fn write_padded(
    out: &mut String,
    text: &str,
    display_len: usize,
    width: usize,
    left_align: bool,
) {
    let pad = width.saturating_sub(display_len);
    if left_align {
        out.push_str(text);
        for _ in 0..pad {
            out.push(' ');
        }
    } else {
        for _ in 0..pad {
            out.push(' ');
        }
        out.push_str(text);
    }
}

pub fn write_separator(out: &mut String, ch: char, widths: &[usize]) {
    for (i, &w) in widths.iter().enumerate() {
        if i > 0 {
            out.push(ch);
            out.push('+');
            out.push(ch);
        }
        for _ in 0..w {
            out.push(ch);
        }
    }
    out.push('\n');
}

/// Drop the trailing padding of the row that starts at byte offset `start`,
/// so lines don't end in whitespace.
pub fn trim_trailing_spaces(out: &mut String, start: usize) {
    let trimmed = out[start..].trim_end_matches(' ').len();
    out.truncate(start + trimmed);
}

/// Render a metric value for the column named `col`.
///
/// `tier` columns show the tier's name rather than its index; everything else
/// goes through the metric's configured [`MetricFormat`], falling back to a
/// plain integer / 2-decimal rendering.
pub fn format_cell_value(
    v: f64,
    col: &str,
    formats: &BTreeMap<String, MetricFormat>,
    tier_names: &[String],
) -> String {
    if let Ok(MetricView::TierIndex { .. }) = col.parse::<MetricView>() {
        let idx = v as usize;
        return tier_names
            .get(idx)
            .cloned()
            .unwrap_or_else(|| format!("T{idx}"));
    }
    match formats.get(col) {
        Some(f) => f.format_value(v),
        None => format_plain(v),
    }
}

pub fn format_plain(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    }
}

/// Resolve each column name to the [`MetricFormat`] its metric is configured
/// with. Columns that don't parse as a [`MetricView`], or whose metric has no
/// format, are simply absent from the map.
pub fn build_format_map(
    metric_cols: &[String],
    metrics_config: Option<&MetricsConfig>,
) -> BTreeMap<String, MetricFormat> {
    let mut map: BTreeMap<String, MetricFormat> = BTreeMap::new();
    let Some(config) = metrics_config else {
        return map;
    };
    for col in metric_cols {
        let Ok(view) = col.parse::<MetricView>() else {
            continue;
        };
        if let Some(fmt) = config.format_for_view(&view) {
            map.insert(col.clone(), fmt.clone());
        }
    }
    map
}
