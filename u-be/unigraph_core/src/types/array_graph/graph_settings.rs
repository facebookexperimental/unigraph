// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Result;
use anyhow::bail;
use ts_rs::TS;

use crate::types::NodeName;
use crate::types::TierName;

#[derive(serde::Serialize, serde::Deserialize, TS, Clone, Default)]
#[ts(export)]
pub struct GraphSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub metric_settings: Option<BTreeMap<String, MetricSettings>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_settings: Option<ArrayGraphUISettings>,
}

#[derive(serde::Serialize, serde::Deserialize, TS, Clone)]
#[ts(export)]
pub struct MetricSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub format: Option<MetricFormat>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    /// Hide table column that displays the metric itself.
    pub column_hide_self: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    /// Column that displays transitive value for the metric.
    pub column_show_transitive: Option<IndividualOptionEnabled>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub column_show_tiered: Option<BTreeMap<TierName, IndividualOptionEnabled>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub show_conjoint_self: Option<IndividualOptionEnabled>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub show_conjoint_tiered: Option<BTreeMap<TierName, IndividualOptionEnabled>>,
}

#[derive(serde::Serialize, serde::Deserialize, TS, Clone)]
#[ts(export)]
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
    SizeBytes {
        /// Configures the unit format for the size metric, units can be variable or forced (kB/MB/GB)
        config: Option<SizeConfig>,
    },
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

#[derive(serde::Serialize, serde::Deserialize, TS, Clone, Copy)]
#[ts(export)]
/// Configuration for size formatting
pub enum SizeConfig {
    /// Flexible units to display readable sizes, but will units will be inconsistent across sizes with variation
    VariableUnits {},
    /// Forces the units to be in Kilobytes, not to be confused with Kibibytes
    ForcekB {},
    /// Forces the units to be in Megabytes, not to be confused with Mebibytes
    ForceMB {},
    /// Forces the units to be in Gigabytes, not to be confused with Gibibytes
    ForceGB {},
    /// Forces the units to be in Kibibytes. Please consider using ForceKB instead
    /// https://fburl.com/workplace/2bl6qcmn
    ForceKiB {},
    /// Forces the units to be in Mebibytes. Please consider using ForceMB instead
    /// https://fburl.com/workplace/2bl6qcmn
    ForceMiB {},
    /// Forces the units to be in Gigibytes. Please consider using ForceGB instead
    /// https://fburl.com/workplace/2bl6qcmn
    ForceGiB {},
}

#[derive(serde::Serialize, serde::Deserialize, TS, Clone, Copy)]
#[ts(export)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(serde::Serialize, serde::Deserialize, TS, Clone)]
#[ts(export)]
pub struct GraphTableSort {
    pub column_id: String,
    pub order: SortOrder,
}

#[derive(serde::Serialize, serde::Deserialize, TS, Clone, Copy)]
#[ts(export)]
pub enum SidebarPanel {
    None,
    Simulation,
    GraphInfo,
}

#[derive(serde::Serialize, serde::Deserialize, TS, Clone)]
#[ts(export)]
pub struct ArrayGraphUISettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_sidebar_panel: Option<SidebarPanel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub graph_table_sort: Option<GraphTableSort>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub columns: Option<ColumnSettings>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub graph_structure: Option<GraphStructure>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
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
    #[ts(optional)]
    pub entry_points_specified: Option<Vec<NodeName>>,
}

/// Enum that defines how the graph structure is displayed in the UI.
/// e.g. which edges we will be following when visualizing the
/// graph in the tree table.
#[derive(serde::Serialize, serde::Deserialize, TS, Clone, Default)]
#[repr(u8)]
#[ts(export)]
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
#[derive(serde::Serialize, serde::Deserialize, TS, Clone, Default)]
#[ts(export)]
pub enum ArrayGraphUISettingsTreeTableEntryPoints {
    #[default]
    Determine,
    AllReachable,
    Specified,
}

#[derive(serde::Serialize, serde::Deserialize, TS, Clone)]
#[ts(export)]
pub struct ColumnSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    show_parents_count: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    show_transitive_count: Option<IndividualOptionEnabled>,

    #[ts(optional)]
    show_conjoint_count: Option<IndividualOptionEnabled>,

    /// Global setting for showing tiered values for metrics
    /// (if tiers are defined)
    /// It is shown by default, but can be hidden
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hide_tiered: Option<bool>,

    /// Global setting for showing transitive values.
    /// Individual columns will be enabled/disabled based on
    /// their individual settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub show_transitive: Option<bool>,

    /// Global setting for showing conjoint cost values.
    /// Individual columns will be enabled/disabled based on
    /// their individual settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub show_conjoint: Option<bool>,
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
#[derive(serde::Serialize, serde::Deserialize, TS, Clone, Copy, Default)]
#[ts(export)]
pub enum IndividualOptionEnabled {
    #[default]
    WhenEnabledGlobally,
    Never,
}
