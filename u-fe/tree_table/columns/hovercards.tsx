// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { MetricFormat } from "../../__generated__/ts/MetricFormat";
import { DeltaMetricCell, MetricCell } from "./Cells";

/// Both sides of a delta column in one place.
///
/// A delta table shows the right-hand value and its `∆`, and neither cell says
/// what the left graph held — so every column in delta mode hangs this off both
/// of its cells.
///
/// `delta` is the column's own number. Pass it whenever the column does not
/// compute a plain subtraction: tiered and transitive-count deltas skip nodes
/// that are identical on both sides, so their `∆` is deliberately *not*
/// `right - left`. When the two disagree the card shows both and says why,
/// which is the only place that difference is ever visible.
/// [`MetricComparisonHovercard`] with its lookups deferred to hover time.
///
/// `UHoverCard` only *renders* its `content` once the card opens, but the
/// element handed to it is constructed on every row render — so reading the
/// values inline would fetch both sides of every metric for all 60k rows in
/// the table rather than the one under the cursor. Passing thunks costs a
/// closure allocation and defers the rest.
export function LazyMetricComparisonHovercard({
  getLeft,
  getRight,
  getDelta,
  format,
}: {
  getLeft: () => number;
  getRight: () => number;
  getDelta?: () => number;
  format?: MetricFormat;
}) {
  return (
    <MetricComparisonHovercard
      valueLeft={getLeft()}
      valueRight={getRight()}
      delta={getDelta?.()}
      format={format}
    />
  );
}

export function MetricComparisonHovercard({
  valueLeft,
  valueRight,
  delta,
  format,
}: {
  valueLeft: number;
  valueRight: number;
  delta?: number;
  format?: MetricFormat;
}) {
  const plainDifference = valueRight - valueLeft;
  const effectiveDelta = delta ?? plainDifference;
  const isExclusive = delta != null && differs(delta, plainDifference);

  return (
    <div className="flex flex-col gap-2 p-2">
      <table className="table-auto w-full">
        <tbody>
          <tr>
            <td className="text-left">Left (before)</td>
            <td className="text-right">
              <MetricCell value={valueLeft} format={format} />
            </td>
          </tr>
          <tr>
            <td className="text-left">Right (after)</td>
            <td className="text-right">
              <MetricCell value={valueRight} format={format} />
            </td>
          </tr>
          <tr>
            <td className="text-left">Delta</td>
            <td className="text-right font-semibold">
              <DeltaMetricCell value={effectiveDelta} format={format} />
            </td>
          </tr>
          {isExclusive && (
            <tr>
              <td className="text-left text-foreground/60">Plain difference</td>
              <td className="text-right text-foreground/60">
                <MetricCell value={plainDifference} format={format} />
              </td>
            </tr>
          )}
        </tbody>
      </table>
      {isExclusive && <ExclusiveDeltaNote />}
    </div>
  );
}

function ExclusiveDeltaNote() {
  return (
    <p className="text-xs text-foreground/60">
      Delta skips nodes that are identical in both graphs, so it reports what
      this node actually brought in rather than everything it can now reach.
      Plain difference is the raw right minus left.
    </p>
  );
}

/// Sums of thousands of `f64`s taken over two different node sets do not land
/// on the same bits even when they mean the same thing, so an exact `!==` would
/// flag every row.
function differs(a: number, b: number): boolean {
  const scale = Math.max(Math.abs(a), Math.abs(b), 1);
  return Math.abs(a - b) > scale * 1e-9;
}
