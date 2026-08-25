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

/// Props a cell passes straight through to its `<span>`.
///
/// These cells are routinely handed to a Radix `asChild` trigger — a
/// hovercard, a tooltip — which works by cloning the element and merging its
/// own `ref`, `className` and pointer handlers into the child's props. A cell
/// that destructures only what it uses drops all of that on the floor, and the
/// trigger silently does nothing: no error, no missing element, just a
/// hovercard that never opens. Spreading the rest is what makes `asChild` work.
type CellProps = Omit<React.ComponentProps<"span">, "children">;

const CELL_CLASS = "px-4 text-right tabular-nums w-full whitespace-nowrap";

export function MissingMetric({ className, ...rest }: CellProps) {
  return (
    <span {...rest} className={clsx(CELL_CLASS, className)}>
      -
    </span>
  );
}

export function MetricCell({
  value,
  format,
  muted,
  className,
  ...rest
}: {
  value: number;
  format?: MetricFormat;
  // Dim the value when the node is unreachable/excluded from the graph.
  muted?: boolean;
} & CellProps) {
  return (
    <span
      {...rest}
      className={clsx(CELL_CLASS, muted && "text-muted-foreground", className)}
    >
      {formatMetric(value, format)}
    </span>
  );
}

export function DeltaMetricCell({
  value,
  format,
  className,
  ...rest
}: {
  value: number;
  format?: MetricFormat;
} & CellProps) {
  const isPositive = value > 0;
  const isNegative = value < 0;
  const sign = isPositive ? "+" : ""; // Negative sign is included in the number itself

  return (
    <span
      {...rest}
      className={clsx(
        CELL_CLASS,
        isPositive && "font-semibold text-red-600",
        isNegative && "font-semibold text-green-600",
        className,
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
