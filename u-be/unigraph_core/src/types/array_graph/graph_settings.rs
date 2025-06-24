// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use ts_rs::TS;

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
    /// hide the column that displays transitive value for the metric.
    pub column_hide_transitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    /// Hide columns that display transitive tiered value for provided tier names.
    pub column_hide_trantitive_tiered: Option<BTreeSet<TierName>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub show_conjoint_self: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub show_conjoint_tiered: Option<BTreeSet<TierName>>,
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
    ColumnsSettings,
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
    pub show_as_a_flat_list: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub show_as_dominator_tree: Option<bool>,
}

#[derive(serde::Serialize, serde::Deserialize, TS, Clone)]
#[ts(export)]
pub struct ColumnSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    show_parents_count: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    show_transitive_count: Option<bool>,

    #[ts(optional)]
    show_conjoint_count: Option<bool>,
}
