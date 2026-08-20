// Copyright (c) Meta Platforms, Inc. and affiliates.

//! ASCII rendering for delta tables.
//!
//! ```text
//! Delta: entry points
//!
//! node_name | change  | size~transitive | size~transitive@delta
//! ==========+=========+=================+======================
//! app       | METRICS |         2.11 kB |               +120 B
//! ```
//!
//! Columns are assembled into a plain grid and padded generically, so adding a
//! column is one push rather than an edit in four places. Headers are the exact
//! `MetricView` strings, so a header can be pasted straight back into
//! `--sort-by`.

use std::collections::BTreeSet;
use std::fmt::Write;

use unigraph_core::MetricView;
use unigraph_core::NodeDiff;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::MetricFormat;
use unigraph_core::graph_settings::MetricsConfig;
use unigraph_core::graph_settings::SortOrder;

use super::ExploreDeltaArrow;
use super::ExploreDeltaEdge;
use crate::rpc_req::ExploreGraphTarget;
use crate::rpc_req::ascii_table::SORT_ARROW_DISPLAY_LEN;
use crate::rpc_req::ascii_table::describe_selection;
use crate::rpc_req::ascii_table::format_plain;
use crate::rpc_req::ascii_table::sort_arrow;
use crate::rpc_req::ascii_table::trim_trailing_spaces;
use crate::rpc_req::ascii_table::write_padded;
use crate::rpc_req::ascii_table::write_separator;

/// Everything the renderer needs. Grouped into a struct because a table has
/// more knobs than a sane argument list.
pub struct Table<'a> {
    pub target: &'a ExploreGraphTarget,
    pub graph_structure: GraphStructure,
    pub changed_nodes_only: bool,
    pub node: Option<&'a ExploreDeltaArrow>,
    pub arrows: &'a [ExploreDeltaArrow],
    pub total_count: usize,
    pub hidden_unchanged_count: usize,
    pub offset: usize,
    pub sort_by_key: Option<String>,
    pub sort_order: SortOrder,
    pub tier_names: &'a [String],
}

pub fn render(table: &Table<'_>, metrics_config: Option<&MetricsConfig>) -> String {
    let all: Vec<&ExploreDeltaArrow> = table.node.into_iter().chain(table.arrows).collect();
    let columns = build_columns(table, &all, metrics_config);
    let grid = build_grid(&all, &columns);
    let widths = column_widths(&grid);

    let mut out = String::with_capacity(256);
    write_summary(&mut out, table);
    write_row(&mut out, &grid.headers, &grid.left_align, &widths);
    write_separator(&mut out, '=', &widths);

    // The drilled-into node gets its own block above its children.
    let (parent, children) = grid.rows.split_at(usize::from(table.node.is_some()));
    for row in parent {
        write_row(&mut out, row, &grid.left_align, &widths);
        write_separator(&mut out, '-', &widths);
    }
    for row in children {
        write_row(&mut out, row, &grid.left_align, &widths);
    }

    write_footer(&mut out, table);
    out
}

// ── Columns ─────────────────────────────────────────────────────

/// A column is a header, an alignment, and a way to turn one arrow into a cell.
struct Column<'a> {
    header: String,
    left_align: bool,
    cell: Box<dyn Fn(&ExploreDeltaArrow) -> String + 'a>,
}

fn build_columns<'a>(
    table: &'a Table<'a>,
    all: &[&ExploreDeltaArrow],
    metrics_config: Option<&MetricsConfig>,
) -> Vec<Column<'a>> {
    let mut columns = vec![Column {
        header: "node_name".to_string(),
        left_align: true,
        cell: Box::new(|a: &ExploreDeltaArrow| a.name.clone()),
    }];

    if all.iter().any(|a| !a.node_diff.is_empty()) {
        columns.push(Column {
            header: "change".to_string(),
            left_align: true,
            cell: Box::new(|a: &ExploreDeltaArrow| change_label(a.node_diff).to_string()),
        });
    }

    if all.iter().any(|a| !edge_label(a).is_empty()) {
        columns.push(Column {
            header: "edge".to_string(),
            left_align: true,
            cell: Box::new(|a: &ExploreDeltaArrow| edge_label(a).to_string()),
        });
    }

    columns.extend(metric_columns(table, all, metrics_config));

    if all.iter().any(|a| a.skipped > 0) {
        columns.push(Column {
            header: "skipped".to_string(),
            left_align: false,
            cell: Box::new(|a: &ExploreDeltaArrow| a.skipped.to_string()),
        });
    }

    if all
        .iter()
        .any(|a| edge_field(a, |e| e.tag.clone()).is_some())
    {
        columns.push(Column {
            header: "tag".to_string(),
            left_align: true,
            cell: Box::new(|a: &ExploreDeltaArrow| sided_cell(a, |e| e.tag.clone())),
        });
    }

    if all.iter().any(|a| edge_field(a, format_dynamic).is_some()) {
        columns.push(Column {
            header: "dynamic".to_string(),
            left_align: true,
            cell: Box::new(|a: &ExploreDeltaArrow| sided_cell(a, format_dynamic)),
        });
    }

    columns
}

/// Render a per-side edge field, showing `before ► after` when the two sides
/// disagree. Same idiom `EnumMetricView` uses in the UI for a changed enum
/// value — a retag is easy to miss if only the new tag is printed.
fn sided_cell(
    arrow: &ExploreDeltaArrow,
    field: impl Fn(&ExploreDeltaEdge) -> Option<String>,
) -> String {
    let left = arrow.l.as_ref().and_then(&field);
    let right = arrow.r.as_ref().and_then(&field);

    match (left, right) {
        (Some(l), Some(r)) if l != r => format!("{l} ► {r}"),
        // One side absent means the whole edge is added or removed, which the
        // `edge` column already says — no need to repeat it as a transition.
        (Some(v), _) | (_, Some(v)) => v,
        (None, None) => String::new(),
    }
}

/// The field's value on whichever side has it, for deciding whether the column
/// is worth showing at all.
fn edge_field(
    arrow: &ExploreDeltaArrow,
    field: impl Fn(&ExploreDeltaEdge) -> Option<String>,
) -> Option<String> {
    arrow
        .r
        .as_ref()
        .and_then(&field)
        .or_else(|| arrow.l.as_ref().and_then(&field))
}

fn format_dynamic(edge: &ExploreDeltaEdge) -> Option<String> {
    edge.dynamic
        .as_ref()
        .map(|d| format!("{}:{}/{}", d.type_key, d.edge_name, d.branch))
}

fn metric_columns<'a>(
    table: &'a Table<'a>,
    all: &[&ExploreDeltaArrow],
    metrics_config: Option<&MetricsConfig>,
) -> Vec<Column<'a>> {
    let keys: BTreeSet<&String> = all.iter().flat_map(|a| a.metrics.keys()).collect();

    keys.into_iter()
        .map(|key| {
            // Keys are produced by `MetricView::to_string`, so this parses.
            let view: MetricView = key.parse().unwrap_or_else(|_| MetricView::metric(key));
            // A categorical format applied to a difference produces a bogus
            // label, so delta cells of such metrics fall back to plain numbers.
            // Default columns never ask for one; an explicit `--metric` can.
            //
            // `base()` drops the side: format is a property of *what* is
            // measured, so a delta column formats like its right-hand sibling.
            let format = metrics_config
                .filter(|_| {
                    !view.is_delta() || super::metrics::has_meaningful_delta(&view, metrics_config)
                })
                .and_then(|c| c.format_for_view(&view.base()).cloned());
            let key = key.clone();
            let tier_names = table.tier_names;

            Column {
                header: header_with_sort_arrow(table, &key),
                left_align: false,
                cell: Box::new(move |a: &ExploreDeltaArrow| match a.metrics.get(&key) {
                    Some(value) => format_metric(*value, &view, format.as_ref(), tier_names),
                    None => "-".to_string(),
                }),
            }
        })
        .collect()
}

/// Delta cells carry an explicit `+` so a regression and a win are
/// distinguishable at a glance; negatives already print their own sign.
fn format_metric(
    value: f64,
    view: &MetricView,
    format: Option<&MetricFormat>,
    tier_names: &[String],
) -> String {
    if matches!(view, MetricView::TierIndex { .. }) && !view.is_delta() {
        let idx = value as usize;
        return tier_names
            .get(idx)
            .cloned()
            .unwrap_or_else(|| format!("T{idx}"));
    }

    let text = match format {
        Some(f) => f.format_value(value),
        None => format_plain(value),
    };

    if view.is_delta() && value > 0.0 {
        format!("+{text}")
    } else {
        text
    }
}

/// Compact enough to keep the column narrow; the full bitflags are on
/// `node_diff` in the JSON for anyone who needs them.
fn change_label(diff: NodeDiff) -> &'static str {
    if diff.contains(NodeDiff::DOES_NOT_EXIST_IN_L) {
        return "ADDED";
    }
    if diff.contains(NodeDiff::DOES_NOT_EXIST_IN_R) {
        return "REMOVED";
    }
    match (diff.has_changed_edgses(), diff.has_changed_metrics()) {
        (true, true) => "EDGES+METRICS",
        (true, false) => "EDGES",
        (false, true) => "METRICS",
        (false, false) => "",
    }
}

/// What happened to the edge leading to this row. `retagged` covers a tag or
/// dynamic-branch change on an edge that exists in both graphs — invisible in
/// `change`, which describes the node rather than the edge.
fn edge_label(arrow: &ExploreDeltaArrow) -> &'static str {
    match (arrow.l.as_ref(), arrow.r.as_ref()) {
        (None, Some(_)) => "added",
        (Some(_), None) => "removed",
        (Some(l), Some(r)) if l.tag != r.tag || format_dynamic(l) != format_dynamic(r) => {
            "retagged"
        }
        _ => "",
    }
}

// ── Grid ────────────────────────────────────────────────────────

struct Grid {
    headers: Vec<String>,
    left_align: Vec<bool>,
    rows: Vec<Vec<String>>,
}

fn build_grid(all: &[&ExploreDeltaArrow], columns: &[Column<'_>]) -> Grid {
    Grid {
        headers: columns.iter().map(|c| c.header.clone()).collect(),
        left_align: columns.iter().map(|c| c.left_align).collect(),
        rows: all
            .iter()
            .map(|arrow| columns.iter().map(|c| (c.cell)(arrow)).collect())
            .collect(),
    }
}

/// Width of the widest cell in each column. The sort arrow is one display
/// column but three bytes, so headers carrying it are measured by hand.
fn column_widths(grid: &Grid) -> Vec<usize> {
    grid.headers
        .iter()
        .enumerate()
        .map(|(i, header)| {
            let header_width = display_width(header);
            grid.rows
                .iter()
                .filter_map(|row| row.get(i))
                .fold(header_width, |acc, cell| acc.max(cell.len()))
        })
        .collect()
}

fn display_width(text: &str) -> usize {
    match text.strip_suffix(sort_arrow(SortOrder::Desc)) {
        Some(base) => base.len() + SORT_ARROW_DISPLAY_LEN,
        None => match text.strip_suffix(sort_arrow(SortOrder::Asc)) {
            Some(base) => base.len() + SORT_ARROW_DISPLAY_LEN,
            None => text.len(),
        },
    }
}

fn write_row(out: &mut String, cells: &[String], left_align: &[bool], widths: &[usize]) {
    let start = out.len();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            let _ = write!(out, " | ");
        }
        write_padded(out, cell, display_width(cell), widths[i], left_align[i]);
    }
    trim_trailing_spaces(out, start);
    out.push('\n');
}

fn header_with_sort_arrow(table: &Table<'_>, key: &str) -> String {
    if table.sort_by_key.as_deref() == Some(key) {
        format!("{key}{}", sort_arrow(table.sort_order))
    } else {
        key.to_string()
    }
}

// ── Summary / footer ────────────────────────────────────────────

fn write_summary(out: &mut String, table: &Table<'_>) {
    match table.target {
        ExploreGraphTarget::EntryPoints {} => out.push_str("Delta: entry points\n"),
        ExploreGraphTarget::AllNodes {} => out.push_str("Delta: all reachable nodes\n"),
        ExploreGraphTarget::Matching { selection } => {
            let _ = writeln!(
                out,
                "Delta nodes matching {}",
                describe_selection(selection)
            );
        }
        ExploreGraphTarget::Node { name } => {
            let structure = match table.graph_structure {
                GraphStructure::Forward => "forward",
                GraphStructure::Reverse => "reverse",
                GraphStructure::Dominator => "dominator",
            };
            let _ = writeln!(out, "Delta edges: {structure}");
            let _ = writeln!(out, "Delta edges of: {name}");
        }
    }
    if table.changed_nodes_only {
        out.push_str("Mode: changed nodes only\n");
    }
    out.push('\n');
}

fn write_footer(out: &mut String, table: &Table<'_>) {
    let shown = table.arrows.len();
    if table.total_count > shown {
        let _ = write!(
            out,
            "\n(showing {shown} of {} rows, offset {})",
            table.total_count, table.offset
        );
    }
    if table.hidden_unchanged_count > 0 {
        let _ = write!(
            out,
            "\n({} unchanged nodes hidden)",
            table.hidden_unchanged_count
        );
    }
}
