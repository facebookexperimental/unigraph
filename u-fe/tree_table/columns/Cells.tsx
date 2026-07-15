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
    <span className="px-4 text-right tabular-nums w-full whitespace-nowrap">
      -
    </span>
  );
}

export function MetricCell({
  value,
  format,
  muted,
}: {
  value: number;
  format?: MetricFormat;
  // Dim the value when the node is unreachable/excluded from the graph.
  muted?: boolean;
}) {
  return (
    <span
      className={clsx(
        "px-4 text-right tabular-nums w-full whitespace-nowrap",
        muted && "text-muted-foreground",
      )}
    >
      {formatMetric(value, format)}
    </span>
  );
}

export function DeltaMetricCell({
  value,
  format,
}: {
  value: number;
  format?: MetricFormat;
}) {
  const isPositive = value > 0;
  const isNegative = value < 0;
  const sign = isPositive ? "+" : ""; // Negative sign is included in the number itself

  return (
    <span
      className={clsx(
        "px-4 text-right tabular-nums w-full whitespace-nowrap",
        isPositive && "font-semibold text-red-600",
        isNegative && "font-semibold text-green-600",
      )}
    >
      {sign}
      {value === 0 ? "-" : formatMetric(value, format)}
    </span>
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
      <span
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
      </span>
    </UTooltip>
  );
}
