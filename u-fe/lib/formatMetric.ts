// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { MetricFormat } from "../__generated__/ts/MetricFormat";
import type { SizeFormatConfig } from "../__generated__/ts/SizeFormatConfig";
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
  } else if ("Size" in format) {
    return formatSizeBytes(value, format.Size);
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
  } else if ("Enum" in format) {
    const key = Math.round(value);
    return format.Enum.variants[key] ?? key.toString();
  } else {
    const _exhaustive: never = format;
    throw new Error(`Unhandled metric format: ${JSON.stringify(_exhaustive)}`);
  }
}

const UNITS = ["bytes", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
export function formatSizeBytes(
  inputValue: number,
  sizeFormatConfig: SizeFormatConfig,
): string {
  const { input_units, output_units } = sizeFormatConfig;
  const bytesValue = (() => {
    switch (input_units) {
      case "Bytes":
        return inputValue;
      default: {
        const _exhaustive: never = input_units;
        throw new Error(
          `Unhandled size input units: ${JSON.stringify(_exhaustive)}`,
        );
      }
    }
  })();

  const [scaledValue, unit, decimals] = (() => {
    switch (output_units) {
      case "VariableUnits": {
        if (bytesValue === 1) {
          return [1, "byte", 0];
        }
        const absBytes = Math.abs(bytesValue);
        const i = Math.min(
          UNITS.length - 1,
          Math.max(0, Math.floor(Math.log10(absBytes) / 3)),
        );

        const decimals = i === 0 ? 0 : 2;
        return [bytesValue / 1000 ** i, UNITS[i], decimals];
      }
      case "KB": {
        return [bytesValue / 1000, "kB", 2];
      }
      case "MB": {
        return [bytesValue / (1000 * 1000), "MB", 2];
      }
      case "GB": {
        return [bytesValue / (1000 * 1000 * 1000), "GB", 2];
      }
      case "KiB": {
        return [bytesValue / 1024, "KiB", 2];
      }
      case "MiB": {
        return [bytesValue / (1024 * 1024), "MiB", 2];
      }
      case "GiB": {
        return [bytesValue / (1024 * 1024 * 1024), "GiB", 2];
      }
      default: {
        const _exhaustive: never = output_units;
        throw new Error(
          `Unhandled size output units: ${JSON.stringify(_exhaustive)}`,
        );
      }
    }
  })();

  const [min_precision, max_precision] = (() => {
    if (
      sizeFormatConfig.min_precision != null &&
      sizeFormatConfig.max_precision != null
    ) {
      return [
        sizeFormatConfig.min_precision ?? 0,
        sizeFormatConfig.max_precision ?? sizeFormatConfig.max_precision ?? 0,
      ];
    }
    return [decimals, decimals];
  })();

  return (
    formatNumber(
      scaledValue,
      min_precision,
      max_precision,
      sizeFormatConfig.use_delimiter ?? true,
    ) +
    " " +
    unit
  );
}
