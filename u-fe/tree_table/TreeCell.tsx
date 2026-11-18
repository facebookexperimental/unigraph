// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import {
  ArrowRight,
  BadgeInfo,
  ChevronDown,
  ChevronRight,
  RefreshCw,
} from "lucide-react";
import { Fragment } from "react";
import type { Arrow } from "../__generated__/ts/Arrow";
import type { DynamicEdgeInfo } from "../__generated__/ts/DynamicEdgeInfo";
import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import CopyToClipboard from "../components/CopyToClipboard";
import UDialog from "../components/UDialog";
import UHoverCard from "../components/UHoverCard";
import { Badge } from "../components/ui/badge";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useTwinGraph } from "../context/NativeGraphContext";
import { usePlugins } from "../context/PluginsContext";
import { useSelectedPath } from "../context/SelectedPathContext";
import { displayNodeName } from "../lib/utils";
import { nodeEdgesChanged, nodeMetricsChanged } from "../native/NodeDiff";
import type TwinGraph from "../native/TwinGraph";
import { skippedNodeCount } from "../native/TwinArrowUtils";
import { H2, P } from "../Typography";
import type { ArrowDiff } from "./arrowDiff";
import { getArrowDiff, getPresenceColor } from "./arrowDiff";
import NodeInfoDialog from "./NodeInfoDialog";
import type { Row } from "./TreeTableRows";

type Props = {
  row: Row;
  canExpand: boolean;
  onToggleExpand: (expanded: boolean) => void;
  minDepth: number;
  nodeName: string;
  paddingComponent: React.ComponentType<{
    size: number;
    className: string;
    color?: string;
  }>;
};

export default function TreeCell(props: Props) {
  const twinGraph = useTwinGraph();
  const twinArrow = props.row.twinArrow;

  const padding = [];
  const PaddingComponent = props.paddingComponent;
  for (let i = Math.max(props.minDepth - 1, 0); i < props.row.depth; i++) {
    padding.push(
      <PaddingComponent key={i} size={16} color="#333" className="mx-2" />,
    );
  }

  const arrowDiff = getArrowDiff(twinGraph, twinArrow);
  const color = getPresenceColor(arrowDiff);

  let lineThrough = null;
  switch (arrowDiff) {
    case "node_became_unreachable":
    case "single_graph_unreachable": {
      lineThrough = "text-foreground/50 line-through";
    }
  }

  return (
    // Hover reveal is CSS, not state. Mounting the info button on hover put
    // the dialog inside a conditional: leaving the row — or Cmd-Tabbing away —
    // unmounted the button and took the open dialog with it. Keeping both
    // mounted and only changing opacity also drops a re-render per row on
    // every pointer move across a 60k-row table.
    <div
      className={clsx(
        "group flex items-center min-w-0 w-full h-full gap-2",
        color,
      )}
    >
      <span className="unigraph-tree-indent contents" aria-hidden="true">
        {padding}
      </span>
      <RowChevron
        canExpand={props.canExpand}
        row={props.row}
        onToggleExpand={props.onToggleExpand}
      />
      <SkippedNodes twinGraph={twinGraph} twinArrow={twinArrow} />
      <ArrowBadge twinArrow={twinArrow} />
      <span
        title={props.nodeName}
        className={clsx(
          "unigraph-tree-name min-w-0 overflow-hidden text-ellipsis text-nowrap",
          isExcludedInBoth(twinGraph, twinArrow) && "text-foreground/50",
          lineThrough,
        )}
      >
        {props.nodeName}
      </span>
      <NodeNameAfterPlugin twinArrow={twinArrow} />
      <ArrowDiffBadges twinArrow={twinArrow} arrowDiff={arrowDiff} />
      <NodeInfoButton twinArrow={twinArrow} />
      <span className="hidden sm:flex items-center opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
        <CopyToClipboard text={props.nodeName} className="ml-2" />
      </span>
    </div>
  );
}

function NodeNameAfterPlugin({ twinArrow }: { twinArrow: TwinArrow }) {
  const { table_node_name_after_component: Component } = usePlugins();
  if (Component == null) {
    return null;
  }
  return <Component twinArrow={twinArrow} />;
}

/// Opens the full node name, dependency edge and change details on demand.
function NodeInfoButton({ twinArrow }: { twinArrow: TwinArrow }) {
  return (
    <UDialog
      title="Node info"
      // 80% of the viewport. `grid-rows` so the body — not the header — takes
      // the slack, and `minmax(0,1fr)` so it can actually shrink and scroll
      // rather than pushing the dialog past its own height.
      className="h-[80dvh] w-[calc(100vw-1rem)] max-w-[calc(100vw-1rem)] grid-rows-[auto_minmax(0,1fr)] overflow-hidden sm:w-[80vw] sm:max-w-[80vw]"
      trigger={
        <button
          type="button"
          aria-label="Node details"
          className="flex items-center justify-center cursor-pointer shrink-0 sm:opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
          onClick={(e) => e.stopPropagation()}
        >
          <BadgeInfo size={16} aria-hidden="true" />
          <span className="sr-only">Full name and impact details</span>
        </button>
      }
    >
      <NodeInfoDialog twinArrow={twinArrow} />
    </UDialog>
  );
}

function ArrowBadge({ twinArrow }: { twinArrow: TwinArrow }) {
  const l = getBadgeData(twinArrow.l ?? null);
  const r = getBadgeData(twinArrow.r ?? null);

  if (l != null && r != null) {
    const areEqual = l.t === r.t && l.label === r.label;

    if (areEqual) {
      return <EdgeBadge label={l.label} badgeType={l.t} dynamic={l.dynamic} />;
    } else {
      return (
        <span className="flex">
          <EdgeBadge label={l.label} badgeType={l.t} dynamic={l.dynamic} />
          <ArrowRight size={16} className="me-2" />
          <EdgeBadge label={r.label} badgeType={r.t} dynamic={r.dynamic} />
        </span>
      );
    }
  } else if (l != null) {
    return <EdgeBadge label={l.label} badgeType={l.t} dynamic={l.dynamic} />;
  } else if (r != null) {
    return <EdgeBadge label={r.label} badgeType={r.t} dynamic={r.dynamic} />;
  } else {
    return null;
  }
}

function EdgeBadge({
  label,
  badgeType,
  dynamic,
}: {
  label: string;
  badgeType?: BadgeType;
  dynamic?: DynamicEdgeInfo;
}) {
  const color = badgeType === "tag" ? "bg-green-800" : "bg-orange-800";
  const badge = (
    <Badge className={clsx("me-2 text-xs py-0 px-0.5", color)}>{label}</Badge>
  );

  if (dynamic != null) {
    return (
      <UHoverCard asChild content={<DynamicEdgeHovercard dynamic={dynamic} />}>
        {badge}
      </UHoverCard>
    );
  }

  return badge;
}

function DynamicEdgeHovercard({ dynamic }: { dynamic: DynamicEdgeInfo }) {
  const metadata = Object.entries(dynamic.metadata ?? {});
  return (
    <div className="flex flex-col gap-2">
      <H2 text="Dynamic edge" />
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
        <dt className="text-foreground/60">Type key</dt>
        <dd className="break-words">{dynamic.type_key}</dd>
        <dt className="text-foreground/60">Edge name</dt>
        <dd className="break-words">{dynamic.edge_name}</dd>
        <dt className="text-foreground/60">Branch</dt>
        <dd className="break-words">{dynamic.branch}</dd>
        {metadata.map(([key, value]) => (
          <Fragment key={key}>
            <dt className="text-foreground/60 break-words">{key}</dt>
            <dd className="break-words">{value}</dd>
          </Fragment>
        ))}
      </dl>
    </div>
  );
}

type BadgeType = "tag" | "dyn";
function getBadgeData(
  arrow: Arrow | null,
): { t: BadgeType; label: string; dynamic?: DynamicEdgeInfo } | null {
  if (arrow == null) return null;

  const tag = arrow.tag;
  const dynamic = arrow.dynamic;
  if (tag != null) {
    return { t: "tag", label: tag };
  } else if (dynamic != null) {
    return {
      t: "dyn",
      label: `${dynamic.type_key}:${dynamic.edge_name}:${dynamic.branch}`,
      dynamic,
    };
  }

  return null;
}

function isExcludedInBoth(twinGraph: TwinGraph, twinArrow: TwinArrow) {
  if (twinGraph.isDeltaGraph()) {
    return (
      twinArrow.l &&
      twinArrow.l.excluded === true &&
      twinArrow.r &&
      twinArrow.r.excluded === true
    );
  } else {
    return twinArrow.r && twinArrow.r.excluded === true;
  }
}

function ArrowDiffBadges({
  twinArrow,
  arrowDiff,
}: {
  twinArrow: TwinArrow;
  arrowDiff: ArrowDiff;
}) {
  const diff = twinArrow.node_diff;

  const badges = [];

  if (arrowDiff === "node_became_reachable") {
    badges.push(
      <UHoverCard
        key="node_became_reachable"
        content="This node was added to the graph."
      >
        <RowBadge text="added" className="bg-added" />
      </UHoverCard>,
    );
  } else if (arrowDiff === "node_became_unreachable") {
    badges.push(
      <UHoverCard
        key="node_became_unreachable"
        content="This node was removed from the graph."
      >
        <RowBadge text="removed" className="bg-removed" />
      </UHoverCard>,
    );
  }

  if (nodeEdgesChanged(diff)) {
    badges.push(
      <RowBadge
        key="edges_changed"
        text="edges changed"
        className="bg-accent"
      />,
    );
  }

  if (nodeMetricsChanged(diff)) {
    badges.push(
      <RowBadge
        key="metrics_changed"
        text="metrics changed"
        className="bg-accent"
      />,
    );
  }

  return badges;
}

function SkippedNodes({
  twinGraph,
  twinArrow,
}: {
  twinGraph: TwinGraph;
  twinArrow: TwinArrow;
}) {
  const selectedPath = useSelectedPath();
  const [graphSettings, setGraphSettings] = useGraphSettings();

  if (twinGraph.l == null) {
    return null;
  }

  const onClick = () => {
    selectedPath.setSelectedPath([twinArrow.points_to], true);
    setGraphSettings({
      ...graphSettings,
      ui_settings: {
        ...graphSettings.ui_settings,
        show_changed_nodes_only: undefined,
      },
    });
  };

  const skipped = skippedNodeCount(twinArrow);

  if (skipped > 0) {
    return (
      <UHoverCard
        asChild
        content={
          <SkippedNodesHovercardContent
            twinGraph={twinGraph}
            twinArrow={twinArrow}
            skipped={skipped}
          />
        }
      >
        <span
          className="bg-primary text-background text-xs py-0.5 px-3 me-1 rounded-lg cursor-pointer"
          onClick={onClick}
        >
          {`+${skipped}`}
        </span>
      </UHoverCard>
    );
  }

  return null;
}

function SkippedNodesHovercardContent({
  twinGraph,
  twinArrow,
  skipped,
}: {
  twinGraph: TwinGraph;
  twinArrow: TwinArrow;
  skipped: number;
}) {
  const fromName = displayNodeName(
    twinGraph.getNodeName(twinArrow.points_from),
  );
  const toName = displayNodeName(twinGraph.getNodeName(twinArrow.points_to));

  return (
    <div className="flex flex-col gap-2">
      <H2 text={`Skipped ${skipped} nodes`} />
      <P text="You are currently comparing two graphs in a 'changed nodes only' mode." />
      <P
        text={`There are ${skipped} nodes between "${fromName}" and "${toName}" that were skipped to reduce clutter.`}
      />
      <P
        text={`Clicking on the badge will disable this mode and show the full (shortest) path to "${toName}".`}
      />
    </div>
  );
}

function RowChevron({
  canExpand,
  row,
  onToggleExpand,
}: {
  canExpand: boolean;
  row: Row;
  onToggleExpand: (expanded: boolean) => void;
}) {
  if (!canExpand) {
    return <span className="size-11 shrink-0" />;
  }
  if (row.isCycle) {
    return (
      <RefreshCw size={16} className="mx-3.5 shrink-0" aria-label="Cycle" />
    );
  }
  const Chevron = row.expanded ? ChevronDown : ChevronRight;
  return (
    <button
      type="button"
      aria-label={
        row.expanded ? "Collapse dependencies" : "Expand dependencies"
      }
      aria-expanded={row.expanded}
      className="size-11 shrink-0 flex items-center justify-center cursor-pointer rounded focus-visible:outline-2"
      onClick={() => onToggleExpand(!row.expanded)}
    >
      <Chevron size={16} aria-hidden="true" />
    </button>
  );
}

function RowBadge({ text, className }: { text: string; className?: string }) {
  return (
    <span
      key="added"
      className={clsx(
        "text-xs py-0.5 px-2 me-1 rounded-lg border border-accent-foreground/50",
        className,
      )}
    >
      {text}
    </span>
  );
}
