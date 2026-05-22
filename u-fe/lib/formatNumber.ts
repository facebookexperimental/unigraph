// Copyright (c) Meta Platforms, Inc. and affiliates.

export default function formatNumber(
  value: number,
  minPrecision = 0,
  maxPrecision = 2,
  shouldUseDelimiter = true,
): string {
  if (Number.isNaN(value)) {
    return "NaN";
  }
  if (value === Number.POSITIVE_INFINITY) {
    return "Infinity";
  }
  if (value === Number.NEGATIVE_INFINITY) {
    return "-Infinity";
  }

  const options: Intl.NumberFormatOptions = {
    minimumFractionDigits: minPrecision,
    maximumFractionDigits: maxPrecision,
    useGrouping: shouldUseDelimiter,
  };

  return new Intl.NumberFormat("en-US", options).format(value);
}
