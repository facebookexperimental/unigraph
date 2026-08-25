// Copyright (c) Meta Platforms, Inc. and affiliates.

/**
 * Label / left / right rows, with the differing ones coloured.
 *
 * Every comparison in the node dialog is the same shape, and stacking the two
 * sides vertically — "Edge (left)" above "Edge (right)" — makes the reader hold
 * six values in their head to find the one that moved. One row per field, both
 * sides on it, changed rows tinted, is the whole idea.
 *
 * Falls back to a single value column outside delta mode, so callers do not
 * need two layouts.
 */

import { Fragment } from "react";
import { cn } from "../lib/utils";

export interface CompareRow {
  label: string;
  left: React.ReactNode;
  right: React.ReactNode;
  /// Whether to tint this row. Compared by the caller, which knows the
  /// underlying values — the rendered nodes are not comparable.
  changed?: boolean;
}

export function CompareTable({
  rows,
  isDelta,
  leftLabel = "Left (before)",
  rightLabel = "Right (after)",
}: {
  rows: readonly CompareRow[];
  isDelta: boolean;
  leftLabel?: string;
  rightLabel?: string;
}) {
  if (rows.length === 0) {
    return <div className="text-foreground/40 italic">none</div>;
  }

  if (!isDelta) {
    return (
      <dl className="grid grid-cols-[minmax(0,auto)_minmax(0,1fr)] gap-x-3 gap-y-0.5 font-mono">
        {rows.map((row) => (
          <Fragment key={row.label}>
            <dt className="text-foreground/60 break-all">{row.label}</dt>
            <dd className="break-all">{row.right}</dd>
          </Fragment>
        ))}
      </dl>
    );
  }

  return (
    <div className="grid grid-cols-[minmax(0,auto)_minmax(0,1fr)_minmax(0,1fr)] font-mono">
      <div />
      <HeaderCell text={leftLabel} className="border-border/50 border-r" />
      <HeaderCell text={rightLabel} />

      {rows.map((row) => (
        <Fragment key={row.label}>
          <div className="text-foreground/60 py-0.5 pr-3 break-all">
            {row.label}
          </div>
          <ValueCell
            value={row.left}
            className={cn(
              "border-border/50 border-r",
              row.changed === true && "bg-red-500/10",
            )}
          />
          <ValueCell
            value={row.right}
            className={cn(row.changed === true && "bg-green-500/10")}
          />
        </Fragment>
      ))}
    </div>
  );
}

/// A row whose two sides are plain strings — the common case, and the one
/// where "did it change" is just `!==`.
export function textRow(
  label: string,
  left: string | null,
  right: string | null,
): CompareRow {
  return {
    label,
    left: left ?? <Absent />,
    right: right ?? <Absent />,
    changed: left !== right,
  };
}

function HeaderCell({ text, className }: { text: string; className?: string }) {
  return (
    <div
      className={cn(
        "text-foreground/50 px-2 pb-0.5 text-[10px] font-semibold tracking-wider uppercase",
        className,
      )}
    >
      {text}
    </div>
  );
}

function ValueCell({
  value,
  className,
}: {
  value: React.ReactNode;
  className?: string;
}) {
  return <div className={cn("px-2 py-0.5 break-all", className)}>{value}</div>;
}

export function Absent() {
  return <span className="text-foreground/30">—</span>;
}
