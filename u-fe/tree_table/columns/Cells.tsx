// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import type { MetricFormat } from "../../__generated__/ts/MetricFormat";
import UTooltip from "../../components/UTooltip";
import formatMetric from "../../lib/formatMetric";

export const NO_PRECISION_FORMAT: MetricFormat = {
  NumberWithVariablePrecision: {
    min_precision: 0,
    max_precision: 0,
    use_delimiter: true,
  },
};

export function MissingMetric() {
  return (
    <p className="px-4 text-right tabular-nums w-full whitespace-nowrap">-</p>
  );
}

export function MetricCell({
  value,
  format,
}: {
  value: number;
  format?: MetricFormat;
}) {
  return (
    <p className="px-4 text-right tabular-nums w-full whitespace-nowrap">
      {formatMetric(value, format)}
    </p>
  );
}

export function WouldBeDeltaMetricCell({
  value,
  format,
}: {
  value: number;
  format?: MetricFormat;
}) {
  let sign = "";
  if (value < 0) {
    sign = "-";
  } else {
    sign = "+";
  }

  return (
    <UTooltip
      tooltip={
        "This node is not reachable in the current graph. This value represents how much the value for the whole graph would change if this edge was included."
      }
    >
      <p
        className={clsx(
          "px-2 mx-2 py-1 text-right tabular-nums w-full whitespace-nowrap",
          {
            "text-green-500": value < 0,
            "text-red-500": value >= 0,
          },
        )}
      >
        {sign}
        {formatMetric(value, format)}
      </p>
    </UTooltip>
  );
}
