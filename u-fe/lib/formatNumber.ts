// Copyright (c) Meta Platforms, Inc. and affiliates.

export function formatPlainNumber(
  number: number,
  decimalPlaces = 2,
  useDelimitor = true,
) {
  if (Number.isNaN(number)) {
    return "NaN";
  }
  if (number === Number.POSITIVE_INFINITY) {
    return "Infinity";
  }
  if (number === Number.NEGATIVE_INFINITY) {
    return "-Infinity";
  }

  const options: Intl.NumberFormatOptions = {
    minimumFractionDigits: decimalPlaces,
    maximumFractionDigits: decimalPlaces,
  };

  if (useDelimitor) {
    options.useGrouping = true;
  } else {
    options.useGrouping = false;
  }

  return new Intl.NumberFormat("en-US", options).format(number);
}
