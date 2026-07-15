// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Result;
use anyhow::bail;

use crate::types::NodeName;

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
pub struct GraphSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Which metric views exist at all (availability layer).
    /// See `MetricsConfig` for details and examples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_config: Option<MetricsConfig>,

    /// Per-view UI visibility overrides keyed by `MetricView.to_string()`.
    /// Controls which available views are shown/hidden by default.
    /// Views not listed here use their type-specific default
    /// (non-dominated → shown, dominated → shown in dominator mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_visibility: Option<BTreeMap<String, MetricViewVisibility>>,

    /// UI presentation settings (columns, sort, sidebar, entry points).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_settings: Option<ArrayGraphUISettings>,
}

/// Controls when a metric view column is shown in the UI.
///
/// This is the visibility layer — it only applies to views that are
/// already available (per `MetricsConfig`). Availability and visibility
/// are separate concerns.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    PartialEq,
    unigraph_delta::Deltable
)]
#[deltable(replace)]
pub enum MetricViewVisibility {
    /// Show when the relevant global toggle is on.
    Enabled,
    /// Show only in dominator graph structure mode (and global toggle is on).
    EnabledInDominatorMode,
    /// Never show (but available — user can toggle on).
    Hidden,
}

/// Whether a metric view type can be computed in this graph.
///
/// Part of `MetricsConfig` — the data-level availability layer.
/// `Unavailable` means the view doesn't exist at all: it won't appear
/// in `available_metric_views()`, the `about` RPC, or the CLI.
/// Defaults to `Available` so that old graphs without `MetricsConfig`
/// keep all their views.
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
pub enum Availability {
    #[default]
    Available,
    Unavailable,
}

impl Availability {
    pub fn is_available(self) -> bool {
        matches!(self, Availability::Available)
    }
}

/// Per-metric configuration: which derived view types are available,
/// plus format and description shared by all views of this metric.
///
/// Each metric in a graph (e.g. `"size"`, `"build_time"`) can produce
/// up to 5 kinds of views:
///
/// - **self_view** — the raw per-node value (e.g. this file is 42 KB)
/// - **transitive** — DFS sum over forward edges (total reachable cost)
/// - **dominated** — DFS sum over the dominator tree (uniquely owned cost)
/// - **tiered** — transitive sum broken down by loading tier (e.g. eager/lazy)
/// - **tiered_dominated** — dominated sum broken down by tier
///
/// All fields are optional. `None` means "inherit from the global default
/// in `MetricsConfig`." This lets you set a project-wide policy and only
/// override specific metrics.
///
/// `format` and `description` are shared across all views of this metric
/// (the format for `size~transitive` is the same as `size` — both are bytes).
///
/// ```text
/// // "size" metric: only show tiered views, hide everything else
/// MetricConfig {
///     self_view:        Some(Unavailable),
///     transitive:       Some(Unavailable),
///     dominated:        None,              // inherits global default
///     tiered:           Some(Available),
///     tiered_dominated: Some(Available),
///     format:           Some(Size { .. }),
///     description:      Some("File size in bytes"),
/// }
///
/// // "impact_count": precomputed value, derived views make no sense
/// MetricConfig {
///     self_view:        Some(Available),
///     transitive:       Some(Unavailable),
///     dominated:        Some(Unavailable),
///     tiered:           Some(Unavailable),
///     tiered_dominated: Some(Unavailable),
///     ..
/// }
/// ```
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
#[deltable(replace)]
pub struct MetricConfig {
    /// The raw per-node value. Hide when only tiered views matter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_view: Option<Availability>,

    /// Transitive sum over forward edges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transitive: Option<Availability>,

    /// Transitive sum over the dominator tree (uniquely owned cost).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dominated: Option<Availability>,

    /// Transitive sum broken down by loading tier (one column per tier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered: Option<Availability>,

    /// Dominated sum broken down by loading tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_dominated: Option<Availability>,

    /// Display format inherited by all views of this metric.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<MetricFormat>,

    /// Human-readable description of what this metric measures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Global defaults for which view types are available per metric.
///
/// Resolution: per-metric field → this default → hardcoded `Available`.
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
#[deltable(replace)]
pub struct DefaultAvailability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_view: Option<Availability>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub transitive: Option<Availability>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub dominated: Option<Availability>,

    /// Set to `Unavailable` to suppress tier columns for all metrics
    /// unless individually overridden in `MetricConfig`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered: Option<Availability>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_dominated: Option<Availability>,
}

/// Global defaults for view type visibility.
///
/// Resolution: per-view override in `metrics_visibility` →
/// per-type field → `all` → hardcoded (dominated → `EnabledInDominatorMode`,
/// everything else → `Enabled`).
///
/// ```text
/// // "Hide everything by default, only show what's explicitly enabled"
/// DefaultVisibility { all: Some(Hidden), .. }
///
/// // "Hide tiered, show everything else normally"
/// DefaultVisibility { tiered: Some(Hidden), tiered_dominated: Some(Hidden), .. }
///
/// // "Hide everything except tiered"
/// DefaultVisibility { all: Some(Hidden), tiered: Some(Enabled), .. }
/// ```
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
#[deltable(replace)]
pub struct DefaultVisibility {
    /// Catch-all default. Lowest precedence — overridden by any
    /// per-type field below.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<MetricViewVisibility>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_view: Option<MetricViewVisibility>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub transitive: Option<MetricViewVisibility>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub dominated: Option<MetricViewVisibility>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered: Option<MetricViewVisibility>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_dominated: Option<MetricViewVisibility>,
}

/// Graph-builder-authored metric configuration.
///
/// Controls both which metric views exist (availability) and which
/// are shown by default (visibility). Per-view visibility overrides
/// live in `GraphSettings.metrics_visibility`.
///
/// # Resolution chains
///
/// **Availability** (does this view exist?):
///   1. `metrics["size"].tiered` (per-metric)
///   2. `default_availability.tiered` (global default)
///   3. hardcoded `Available`
///
/// **Visibility** (is this view shown by default?):
///   1. `GraphSettings.metrics_visibility["size#eager"]` (per-view override)
///   2. `default_visibility.tiered` (global default)
///   3. hardcoded: dominated → `EnabledInDominatorMode`, else → `Enabled`
///
/// # Example
///
/// ```text
/// MetricsConfig {
///     default_availability: DefaultAvailability {
///         tiered: Some(Unavailable),  // no tier columns except overrides
///     },
///     default_visibility: DefaultVisibility {
///         dominated: Some(Hidden),    // dominated hidden by default
///     },
///     metrics: {
///         "size": MetricConfig {
///             tiered: Some(Available),  // override: size gets tiers
///             self_view: Some(Unavailable),
///         },
///     },
/// }
/// ```
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
#[deltable(replace)]
pub struct MetricsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_availability: Option<DefaultAvailability>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_visibility: Option<DefaultVisibility>,

    /// Per-metric configuration keyed by metric name (e.g. `"size"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<BTreeMap<String, MetricConfig>>,

    // ── Structural (non-metric) view availability ────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parents_count: Option<Availability>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub count_transitive: Option<Availability>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub count_dominated: Option<Availability>,
}

impl MetricsConfig {
    pub fn format_for(&self, metric_name: &str) -> Option<&MetricFormat> {
        self.metrics
            .as_ref()
            .and_then(|m| m.get(metric_name))
            .and_then(|mc| mc.format.as_ref())
    }

    /// Look up format for any MetricView. Derived views (transitive,
    /// dominated, tiered) inherit format from their base metric name.
    pub fn format_for_view(&self, view: &crate::MetricView) -> Option<&MetricFormat> {
        view.metric_name().and_then(|name| self.format_for(name))
    }

    pub fn description_for(&self, metric_name: &str) -> Option<&str> {
        self.metrics
            .as_ref()
            .and_then(|m| m.get(metric_name))
            .and_then(|mc| mc.description.as_deref())
    }

    pub(crate) fn resolve_availability(
        &self,
        name: &str,
        per_metric: fn(&MetricConfig) -> Option<Availability>,
        default_field: fn(&DefaultAvailability) -> Option<Availability>,
    ) -> Availability {
        if let Some(mc) = self.metrics.as_ref().and_then(|m| m.get(name)) {
            if let Some(avail) = per_metric(mc) {
                return avail;
            }
        }
        self.default_availability
            .as_ref()
            .and_then(default_field)
            .unwrap_or(Availability::Available)
    }
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq
)]
/// Value that defines how to format metric values (in the UI or CLI output)
/// This value is cross platform enum type which is represented as an object/shape
/// with all keys/properties optional and expected to have exactly ONE key/property
/// to be set at runtime.
pub enum MetricFormat {
    /// a value representing a percentage value.
    Percent {
        /// If set to true, the value is expected to be divided by 100.
        /// e.g. 0 => 0%, 1.0 => 100%, 0.24 => 24%
        scaled_percentage: Option<bool>,
    },
    /// Given a value of bytes, format it as a size (e.g. 1.4MB, 2kB, etc)
    Size(SizeFormatConfig),
    /// Given a value of 0 or 1, format it as a boolean
    NumericBoolean {},
    /// 1       -> {min:    2, max: 4, delimiter: true}  -> "1.00"
    /// 1.1     -> {min:    2, max: 4, delimiter: true}  -> "1.10"
    /// 1.12    -> {min:    2, max: 4, delimiter: true}  -> "1.12"
    /// 1.123   -> {min:    2, max: 4, delimiter: true}  -> "1.123"
    /// 1.1234  -> {min:    2, max: 4, delimiter: true}  -> "1.1234"
    /// 1.12345 -> {min:    2, max: 4, delimiter: true}  -> "1.1235"
    /// 1000000 -> {min:    2, max: 4, delimiter: true}  -> "1,000,000.00"
    /// 1000000 -> {min:    2, max: 4, delimiter: false} -> "1000000.00"
    /// 1000000 -> {min:    0, max: 0, delimiter: true}  -> "1,000,000"
    NumberWithVariablePrecision {
        min_precision: Option<usize>,
        max_precision: Option<usize>,
        use_delimiter: Option<bool>,
    },
    /// Treat the value as an enum: map an integer value to a display label.
    /// The metric value is coerced to an integer (rounded) before lookup.
    /// e.g. {0 => "root", 1 => "nested", 3 => "bootload"}
    /// Values without a matching label fall back to the raw integer string.
    Enum {
        /// Map from integer value to its display label.
        variants: BTreeMap<i64, String>,
    },
    /// Marks a metric as the START of a timespan (for a tracing/gantt bar).
    /// The paired END value lives in a separate metric named by
    /// `timespan_end_metric_name`. The UI renders a positioned bar spanning
    /// start→end; the CLI and any text context render the raw numeric value.
    TimespanStart {
        /// Name of the metric holding the span END value.
        timespan_end_metric_name: Option<String>,
        /// How the numeric start/end values should be interpreted.
        units: TimespanUnits,
        /// When true, treat `0.0` as "no value" (the default for missing
        /// metrics): such nodes are excluded from the timeline min/max and
        /// render no bar, so a metric-less row doesn't show a stray dot.
        ignore_zero: Option<bool>,
    },
}

/// How timespan metric values are interpreted. Only elapsed seconds today;
/// wall-clock formats (e.g. `UnixTimestamp`) can be added later.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    PartialEq
)]
pub enum TimespanUnits {
    ElapsedSeconds,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq
)]
pub struct SizeFormatConfig {
    /// What is the unit of the input value that will be formatted
    pub input_units: SizeInputUnits,
    /// Configures the unit format for the size metric, units can be variable or forced (kB/MB/GB)
    pub output_units: SizeOutputUnits,

    pub min_precision: Option<usize>,
    pub max_precision: Option<usize>,
    pub use_delimiter: Option<bool>,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    PartialEq
)]
pub enum SizeInputUnits {
    Bytes,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    PartialEq
)]
/// Configuration for size formatting
pub enum SizeOutputUnits {
    /// Flexible units to display readable sizes, but will units will be inconsistent across sizes with variation
    VariableUnits,
    /// Forces the units to be in Kilobytes, not to be confused with Kibibytes
    KB,
    /// Forces the units to be in Megabytes, not to be confused with Mebibytes
    MB,
    /// Forces the units to be in Gigabytes, not to be confused with Gibibytes
    GB,
    /// Forces the units to be in Kibibytes. Please consider using ForceKB instead
    /// https://fburl.com/workplace/2bl6qcmn
    KiB,
    /// Forces the units to be in Mebibytes. Please consider using ForceMB instead
    /// https://fburl.com/workplace/2bl6qcmn
    MiB,
    /// Forces the units to be in Gigibytes. Please consider using ForceGB instead
    /// https://fburl.com/workplace/2bl6qcmn
    GiB,
}

impl MetricFormat {
    pub fn format_value(&self, value: f32) -> String {
        match self {
            MetricFormat::Percent { scaled_percentage } => {
                let pct = if *scaled_percentage == Some(true) {
                    value * 100.0
                } else {
                    value
                };
                format!("{}%", format_number(pct, 0, 2, true))
            }
            MetricFormat::Size(config) => format_size(value, config),
            MetricFormat::NumericBoolean {} => match value as i32 {
                0 => "False".into(),
                1 => "True".into(),
                _ => format!("{value}"),
            },
            MetricFormat::NumberWithVariablePrecision {
                min_precision,
                max_precision,
                use_delimiter,
            } => format_number(
                value,
                min_precision.unwrap_or(0),
                max_precision.unwrap_or(2),
                use_delimiter.unwrap_or(true),
            ),
            MetricFormat::Enum { variants } => {
                let key = value.round() as i64;
                variants
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| format!("{key}"))
            }
            // Timespan rendering (the bar) is UI-only; every text context
            // (CLI, sorting tooltip) just shows the raw numeric value.
            MetricFormat::TimespanStart { .. } => format_number(value, 0, 2, true),
        }
    }
}

fn format_size(value: f32, config: &SizeFormatConfig) -> String {
    let bytes = match config.input_units {
        SizeInputUnits::Bytes => value,
    };
    let (scaled, unit, default_decimals) = match config.output_units {
        SizeOutputUnits::VariableUnits => {
            if bytes.abs() < 1.0 {
                (bytes, "bytes", 0)
            } else {
                let i = (bytes.abs().log10() / 3.0).floor() as usize;
                let i = i.min(8);
                let units = ["bytes", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
                let decimals = if i == 0 { 0 } else { 2 };
                (bytes / 1000f32.powi(i as i32), units[i], decimals)
            }
        }
        SizeOutputUnits::KB => (bytes / 1000.0, "kB", 2),
        SizeOutputUnits::MB => (bytes / 1_000_000.0, "MB", 2),
        SizeOutputUnits::GB => (bytes / 1_000_000_000.0, "GB", 2),
        SizeOutputUnits::KiB => (bytes / 1024.0, "KiB", 2),
        SizeOutputUnits::MiB => (bytes / (1024.0 * 1024.0), "MiB", 2),
        SizeOutputUnits::GiB => (bytes / (1024.0 * 1024.0 * 1024.0), "GiB", 2),
    };
    let min_p = config.min_precision.unwrap_or(default_decimals);
    let max_p = config.max_precision.unwrap_or(default_decimals);
    let delimiter = config.use_delimiter.unwrap_or(true);
    format!("{} {unit}", format_number(scaled, min_p, max_p, delimiter))
}

fn format_number(
    value: f32,
    min_precision: usize,
    max_precision: usize,
    use_delimiter: bool,
) -> String {
    let rounded = if max_precision == 0 {
        value.round()
    } else {
        let factor = 10f32.powi(max_precision as i32);
        (value * factor).round() / factor
    };

    let formatted = format!("{:.prec$}", rounded, prec = max_precision);

    let trimmed = if min_precision < max_precision {
        let dot_pos = formatted.find('.');
        if let Some(dot) = dot_pos {
            let min_end = dot + 1 + min_precision;
            let actual_end = formatted.len();
            let mut end = actual_end;
            while end > min_end && formatted.as_bytes()[end - 1] == b'0' {
                end -= 1;
            }
            if end == dot + 1 && min_precision == 0 {
                &formatted[..dot]
            } else {
                &formatted[..end]
            }
        } else {
            &formatted
        }
    } else {
        &formatted
    };

    if use_delimiter {
        add_thousands_delimiter(trimmed)
    } else {
        trimmed.to_string()
    }
}

fn add_thousands_delimiter(s: &str) -> String {
    let (integer_part, decimal_part) = match s.find('.') {
        Some(dot) => (&s[..dot], Some(&s[dot..])),
        None => (s, None),
    };
    let negative = integer_part.starts_with('-');
    let digits = if negative {
        &integer_part[1..]
    } else {
        integer_part
    };
    let mut result = String::with_capacity(s.len() + digits.len() / 3);
    if negative {
        result.push('-');
    }
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    if let Some(dec) = decimal_part {
        result.push_str(dec);
    }
    result
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    PartialEq
)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    PartialEq,
    unigraph_delta::Deltable
)]
#[deltable(replace)]
pub enum SidebarPanel {
    None,
    Simulation,
    GraphInfo,
    TraversalConfigEditor,
    DebugPanel,
}

/// Enum that defines whether an option is enabled or not depending
/// on whether the right graph is present or not.
/// For example, when we have `changed nodes only` enabled it has no
/// meaning in the context of a single graph. This option provides
/// extra safety to make sure we don't accedentally pass `true` in
/// cases that are invalid.
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
pub enum OptionEnabledDependingOnRightGraph {
    WhenRightGraphPresent,
    #[default]
    Never,
}

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
pub struct ArrayGraphUISettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_sidebar_panel: Option<SidebarPanel>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<ColumnSettings>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_structure: Option<GraphStructure>,

    /// Only used in delta view when comparing two graphs.
    /// This will compress paths that graph table renders
    /// and only show nodes that have changed between the two graphs
    /// while skipping a lot of nodes in between.
    ///
    /// We want to find the CLOSEST (possibly not direct) children
    /// in the transitive dependencies of the node so we can show changed
    /// nodes graph only.
    ///
    /// E.g. if we have two graphs we're comparing:
    ///
    /// A          A
    ///   B          B
    ///     C          C
    ///       D          F    <- D was removed and F was added
    ///
    ///
    /// The actual change is hidden deep down in the node. We would want to skip
    /// showing B because it has no changes, and only show C, D and F because they
    ///
    /// A          A
    ///   C          C
    ///     D          F
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_changed_nodes_only: Option<OptionEnabledDependingOnRightGraph>,

    /// What nodes should we use as the "start" of the graph
    /// when we render the table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_points: Option<ArrayGraphUISettingsTreeTableEntryPoints>,

    /// Used in combination with `entry_points` settings.`
    /// If entry_points is set to `Specified`, this value will be
    /// used to determine entry points. This value is stored separately
    /// so we can preserve selected entry points when switching
    /// between different entry points settings.
    /// E.g. if we're exploring `reverse from a specific node` and want
    /// to hop into `show as flat list`, we want to preserve
    /// the selected entry points, so when we switch back to "reverse"
    /// we keep the same selected entry point
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_points_specified: Option<Vec<NodeName>>,
}

/// Enum that defines how the graph structure is displayed in the UI.
/// e.g. which edges we will be following when visualizing the
/// graph in the tree table.
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
#[repr(u8)]
pub enum GraphStructure {
    // default, follow the forward edges
    #[default]
    Forward = 0,
    // follow the dominator tree edges
    Dominator = 1,
    // follow the reverse edges (child -> parent)
    Reverse = 2,
}

impl GraphStructure {
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(GraphStructure::Forward),
            1 => Ok(GraphStructure::Dominator),
            2 => Ok(GraphStructure::Reverse),
            _ => bail!("Invalid GraphStructure value: {}", value),
        }
    }
}

/// Will be used as entry points for the tree table.
/// Otherwise we will use the determined entry points.
/// This is needed for things like: show as flat list, show selected nodes,
/// show reverse from a specific node, etc.
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
#[deltable(replace)]
pub enum ArrayGraphUISettingsTreeTableEntryPoints {
    #[default]
    Determine,
    AllReachable,
    Specified,
}

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
pub struct ColumnSettings {
    /// Graph table in UI will be sorted using provided column
    /// and order if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_table_sort: Option<GraphTableSort>,

    /// Global setting for showing metric values
    /// (if tiers are defined)
    /// It is shown by default, but can be hidden
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_metrics: Option<bool>,

    /// Global setting for showing tiered values for metrics
    /// (if tiers are defined)
    /// It is hidden by default, but can be enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_tiered_metrics: Option<bool>,

    /// Global setting for showing dominated metric values.
    /// Defaults to showing because individual values default
    /// to only showing when in Dominator mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_dominated_tiered_metrics: Option<bool>,

    /// Global setting for showing columns related to
    /// node counts, like transitive counts or parents counts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_counts: Option<bool>,

    /// Show a column that displays the tier each node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_tier_column: Option<bool>,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    PartialEq,
    unigraph_delta::Deltable
)]
#[deltable(replace)]
pub struct GraphTableSort {
    pub column: SortColumn,
    pub order: SortOrder,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    PartialEq
)]
pub enum SortColumn {
    /// Sort by node name (tree column)
    NodeName {},

    /// Sort by a metric view column. `key` is `MetricView.to_string()`,
    /// optionally suffixed with `@right` or `@delta` to select the
    /// comparison-graph or delta column in twin-graph mode.
    MetricView { key: String },
}

impl Default for SortColumn {
    fn default() -> Self {
        SortColumn::NodeName {}
    }
}

#[cfg(test)]
mod format_value_tests {
    use super::*;

    #[test]
    fn test_enum_format_value() {
        let format = MetricFormat::Enum {
            variants: BTreeMap::from([
                (0, "root".to_string()),
                (1, "nested".to_string()),
                (3, "bootload".to_string()),
            ]),
        };

        // Exact integer values map to their labels.
        assert_eq!(format.format_value(0.0), "root");
        assert_eq!(format.format_value(1.0), "nested");
        assert_eq!(format.format_value(3.0), "bootload");

        // Floats are rounded to the nearest integer before lookup.
        assert_eq!(format.format_value(1.4), "nested");
        assert_eq!(format.format_value(2.6), "bootload"); // rounds to 3

        // Values without a matching label fall back to the raw integer.
        assert_eq!(format.format_value(2.0), "2");
        assert_eq!(format.format_value(9.0), "9");
    }

    #[test]
    fn test_timespan_start_format_value_is_numeric() {
        let format = MetricFormat::TimespanStart {
            timespan_end_metric_name: Some("event_end".to_string()),
            units: TimespanUnits::ElapsedSeconds,
            ignore_zero: Some(true),
        };

        // The bar is UI-only; text contexts render the raw number.
        assert_eq!(format.format_value(0.0), "0");
        assert_eq!(format.format_value(1.5), "1.5");
        assert_eq!(format.format_value(1000.0), "1,000");
    }
}
