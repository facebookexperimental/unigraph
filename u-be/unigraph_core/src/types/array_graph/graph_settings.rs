// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Result;
use anyhow::bail;

use crate::types::NodeName;
use crate::types::TierName;

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Default,
    PartialEq
)]
pub struct GraphSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_settings: Option<ArrayGraphUISettings>,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    PartialEq
)]
pub struct MetricSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<MetricFormat>,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// Hide table column that displays the metric itself.
    pub column_hide_self: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// Column that displays transitive value for the metric.
    pub column_show_transitive: Option<IndividualOptionEnabled>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_show_tiered: Option<BTreeMap<TierName, IndividualOptionEnabled>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_conjoint_tiered: Option<BTreeMap<TierName, IndividualOptionEnabled>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_dominated: Option<IndividualDominatedOptionEnabled>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_dominated_tiered: Option<BTreeMap<TierName, IndividualDominatedOptionEnabled>>,
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
    PartialEq
)]
pub enum SidebarPanel {
    None,
    Simulation,
    GraphInfo,
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
    PartialEq
)]
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
    PartialEq
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
    PartialEq
)]
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
    PartialEq
)]
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
    PartialEq
)]
pub struct ColumnSettings {
    /// Graph table in UI will be sorted using provided column
    /// and order if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_table_sort: Option<GraphTableSort>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_parents_count: Option<IndividualOptionEnabled>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_transitive_count: Option<IndividualOptionEnabled>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_conjoint_count: Option<IndividualOptionEnabled>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_dominated_count: Option<IndividualDominatedOptionEnabled>,

    /// Global setting for showing metric values
    /// (if tiers are defined)
    /// It is shown by default, but can be hidden
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_metrics: Option<bool>,

    /// Global setting for showing tiered values for metrics
    /// (if tiers are defined)
    /// It is hidden by default, but can be endabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_tiered_metrics: Option<bool>,

    /// Global setting for showing conjoint values for tiered metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_conjoint_tiered_metrics: Option<bool>,

    /// Global setting for showing dominated metric values.
    /// Individual columns will be enabled/disabled based on
    /// their individual settings.
    /// defaults to showing because individual values default
    /// to only whowing when in Dominator mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_dominated_tiered_metrics: Option<bool>,

    /// Global setting for showing columns related to
    /// node counts, like transitive counts, parents counts,
    /// or conjoint cost for node counts.
    /// Individual columns will be enabled/disabled based on
    /// their individual settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_counts: Option<bool>,

    /// Show a column that displays the tier each node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_tier_column: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric_settings: Option<BTreeMap<String, MetricSettings>>,
}

/// Enum that defines whether an individual option is enabled or not.
/// This is for cases where we have a global settings that can show/hide certain
/// things plus individual settings for the same thing on each metric/tier/etc.
/// E.g. we have dominator tree and dominated size columns we want to display.
/// We have a single "show dominator tree" button that we can enable/disable.
/// But normally that would add multiple dominated size/count columns. For graphs
/// with many tiers/metrics we're talkinb about 10+ columns, while the user is likely
/// to only care about one or two.
/// For that reason we can add these settings to individual columns that can make them
/// be disabled even when the global setting is enabled.
#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    Default,
    PartialEq
)]
pub enum IndividualOptionEnabled {
    #[default]
    WhenEnabledGlobally,
    Never,
}

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    Clone,
    Copy,
    Default,
    PartialEq
)]
pub enum IndividualDominatedOptionEnabled {
    // This will only show when global flag is enabled and we're in dominator mode.
    #[default]
    WhenEnabledGloballyAndInDominatorMode,
    WhenEnabledGlobally,
    Never,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    PartialEq
)]
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
pub enum ColumnType {
    Left,
    Right,
    Delta,
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

    /// Transitive count column
    TransitiveCount { t: ColumnType },

    /// Transitive dominated count column
    DominatedCount { t: ColumnType },

    /// Number of parents for each node
    ParentsCount { t: ColumnType },

    /// Metric column for specified metric
    Metric { t: ColumnType, name: String },
    /// Metric column for specified metric (Right Graph)

    /// Transitive metric column for specified metric
    TransitiveMetric { t: ColumnType, name: String },

    /// Dominated metric column for specified metric
    DominatedMetric { t: ColumnType, name: String },

    /// Tiered transitive metric column for specified metric
    TieredTransitiveMetric {
        t: ColumnType,
        name: String,
        tier: String,
    },

    /// Conjoint count
    ConjointCount { t: ColumnType },

    /// Conjoint tiered metric
    ConjointTieredMetric {
        t: ColumnType,
        name: String,
        tier: String,
    },

    /// Conjoint tiered metric
    DominatedTieredMetric {
        t: ColumnType,
        name: String,
        tier: String,
    },
}
