// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import type { TierIDX } from "../../native/NodeFlags";
import type TwinGraph from "../../native/TwinGraph";
import type { Row } from "../TreeTableRows";
import type { Column, ColumnsCtx } from "./useGraphTreeTableColumns";
import { NonSortableColumnDefinition } from "../columns";

export class NodeTierColumn implements Column {
  ctx: ColumnsCtx;
  twinGraph: TwinGraph;

  constructor(ctx: ColumnsCtx, twinGraph: TwinGraph) {
    this.ctx = ctx;
    this.twinGraph = twinGraph;
  }

  isEnabled() {
    return (
      this.ctx.graphSettings.ui_settings?.columns?.show_tier_column === true
    );
  }

  getID(): string {
    return "Tier";
  }

  sortable() {
    return null;
  }

  definition(): [string, NonSortableColumnDefinition] {
    const left = this.twinGraph.l;
    const right = this.twinGraph.r;

    const columnID = this.getID();

    const definition: NonSortableColumnDefinition = {
      t: "non_sortable_column",
      label: columnID,
      renderer: (row: Readonly<Row>) => {
        const tierL = left.getNodeTierName(row.twinArrow.points_to);
        const tierR = right?.getNodeTierName(row.twinArrow.points_to) ?? null;

        if (right == null || tierL?.[1] === tierR?.[1]) {
          if (tierL == null) {
            return null;
          }
          return (
            <div className="flex justify-center w-full">
              <TierBadge tier={tierL} />
            </div>
          );
        } else {
          return (
            <div className="flex justify-center w-full">
              <TierBadge tier={tierL} />
              <span className="text-[10px] self-center px-1">►</span>
              <TierBadge tier={tierR} />
            </div>
          );
        }
      },
      isHidden: false,
    };

    return [columnID, definition];
  }
}

function TierBadge({
  className,
  tier,
}: {
  className?: string;
  tier: [string, TierIDX] | null;
}) {
  let bgColor = "border-accent-foreground/50";
  switch (tier?.[1] ?? null) {
    case 0:
      bgColor = "bg-yellow-500/35";
      break;
    case 1:
      bgColor = "bg-blue-500/35";
      break;
    case 2:
      bgColor = "bg-green-500/35";
      break;
    case 3:
      bgColor = "bg-purple-500/35";
      break;
    case null:
      bgColor = "bg-graph-500/50";
      break;
  }

  return (
    <span
      key="added"
      className={clsx(
        "text-xs py-0.5 px-2 rounded-lg border  border-accent-foreground/30",
        bgColor,
        className,
      )}
    >
      {tier?.[0] ?? "none"}
    </span>
  );
}
