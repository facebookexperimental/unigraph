// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { MetricFormat } from "../../__generated__/ts/MetricFormat";
import { DeltaMetricCell, MetricCell } from "./Cells";

export function MetricDeltaRightHovercard({
  valueLeft,
  valueRight,
  format,
}: {
  valueLeft: number;
  valueRight: number;
  format?: MetricFormat;
}) {
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
              <DeltaMetricCell value={valueRight - valueLeft} format={format} />
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
