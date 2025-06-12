// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { MetricFormat } from "u-be/unigraph_core/bindings/MetricFormat";
import type { SizeConfig } from "u-be/unigraph_core/bindings/SizeConfig";
import formatNumber from "./formatNumber";

const DEFAULT_METRIC_FORMAT: MetricFormat = {
  NumberWithVariablePrecision: {
    min_precision: 0,
    max_precision: 2,
    use_delimiter: true,
  },
};
export default function formatMetric(
  value: number,
  format: MetricFormat = DEFAULT_METRIC_FORMAT,
): string {
  if ("Percent" in format) {
    const config = format.Percent;
    let pctValue = value;
    if (config.scaled_percentage === true) {
      pctValue = value * 100;
    }
    return formatNumber(pctValue, 0, 2, true) + "%";
  } else if ("SizeBytes" in format) {
    const sizeConfig = format.SizeBytes.config;
    if (sizeConfig === null) {
      return formatNumber(value, 0, 0, true) + " bytes";
    } else {
      return formatSizeBytes(value, sizeConfig);
    }
  } else if ("NumericBoolean" in format) {
    switch (value) {
      case 0:
        return "False";
      case 1:
        return "True";
      default:
        return value.toString();
    }
  } else if ("NumberWithVariablePrecision" in format) {
    const config = format.NumberWithVariablePrecision;
    return formatNumber(
      value,
      config.min_precision ?? 0,
      config.max_precision ?? 2,
      config.use_delimiter ?? true,
    );
  } else {
    const _exhaustive: never = format;
    throw new Error(`Unhandled metric format: ${JSON.stringify(_exhaustive)}`);
  }
}

const UNITS = ["bytes", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
export function formatSizeBytes(value: number, sizeConfig: SizeConfig): string {
  const [scaledValue, unit, decimals] = (() => {
    if ("VariableUnits" in sizeConfig) {
      if (value === 1) {
        return [1, "byte", 0];
      }
      const absBytes = Math.abs(value);
      const i = Math.min(
        UNITS.length - 1,
        Math.max(0, Math.floor(Math.log10(absBytes) / 3)),
      );

      const decimals = i === 0 ? 0 : 2;
      return [value / 1000 ** i, UNITS[i], decimals];
    } else if ("ForcekB" in sizeConfig) {
      return [value / 1000, "kB", 2];
    } else if ("ForceMB" in sizeConfig) {
      return [value / (1000 * 1000), "MB", 2];
    } else if ("ForceGB" in sizeConfig) {
      return [value / (1000 * 1000 * 1000), "GB", 2];
    } else if ("ForceKiB" in sizeConfig) {
      return [value / 1024, "KiB", 2];
    } else if ("ForceMiB" in sizeConfig) {
      return [value / (1024 * 1024), "MiB", 2];
    } else if ("ForceGiB" in sizeConfig) {
      return [value / (1024 * 1024 * 1024), "GiB", 2];
    } else {
      const _exhaustive: never = sizeConfig;
      throw new Error(`Unhandled size config: ${JSON.stringify(_exhaustive)}`);
    }
  })();

  return formatNumber(scaledValue, decimals, decimals, true) + " " + unit;
}
