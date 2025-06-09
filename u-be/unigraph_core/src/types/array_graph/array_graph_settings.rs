use std::collections::BTreeMap;

use ts_rs::TS;
#[derive(serde::Serialize, TS)]
#[ts(export)]
pub struct ArrayGraphSettings {
    pub metric_settings: Option<BTreeMap<String, MetricSettings>>,
}

#[derive(serde::Serialize, TS)]
#[ts(export)]
pub struct MetricSettings {
    pub description: String,
    pub format: Option<MetricFormat>,
}
#[derive(serde::Serialize, TS)]
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

#[derive(serde::Serialize, TS)]
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
