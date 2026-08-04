// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import {
  ArrowRight,
  BadgeInfo,
  ChevronDown,
  ChevronRight,
  RefreshCw,
  Wrench,
} from "lucide-react";
import { Fragment, useState } from "react";
import type { Arrow } from "../__generated__/ts/Arrow";
import type { DynamicEdgeInfo } from "../__generated__/ts/DynamicEdgeInfo";
import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import CopyToClipboard from "../components/CopyToClipboard";
import UDialog from "../components/UDialog";
import UHoverCard from "../components/UHoverCard";
import { Badge } from "../components/ui/badge";
import { useDebugMode } from "../context/DebugModeContext";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useTwinGraph } from "../context/NativeGraphContext";
import { usePlugins } from "../context/PluginsContext";
import { useSelectedPath } from "../context/SelectedPathContext";
import { displayNodeName } from "../lib/utils";
import { nodeEdgesChanged, nodeMetricsChanged } from "../native/NodeDiff";
import type TwinGraph from "../native/TwinGraph";
import { H2, P } from "../Typography";
import NodeDebugDialog from "./NodeDebugDialog";
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
  const [debugMode] = useDebugMode();
  const twinGraph = useTwinGraph();
  const twinArrow = props.row.twinArrow;
  const [isHovered, setIsHovered] = useState(false);

  const padding = [];
  const PaddingComponent = props.paddingComponent;
  for (let i = Math.max(props.minDepth - 1, 0); i < props.row.depth; i++) {
    padding.push(
      <PaddingComponent key={i} size={16} color="#333" className="mx-2" />,
    );
  }

  const color = getPresenceColor(twinGraph, twinArrow);
  const arrowDiff = getArrowDiff(twinGraph, twinArrow);

  let lineThrough = null;
  switch (arrowDiff) {
    case "node_became_unreachable":
    case "single_graph_unreachable": {
      lineThrough = "text-foreground/50 line-through";
    }
  }

  return (
    <div
      className={clsx("flex items-center w-full h-full gap-2", color)}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {padding}
      <RowChevron
        canExpand={props.canExpand}
        row={props.row}
        onToggleExpand={props.onToggleExpand}
      />
      <SkippedNodes twinGraph={twinGraph} twinArrow={twinArrow} />
      <ArrowBadge twinArrow={twinArrow} />
      <span
        className={clsx(
          "min-w-0 overflow-hidden text-ellipsis text-nowrap",
          isExcludedInBoth(twinGraph, twinArrow) && "text-foreground/50",
          lineThrough,
        )}
      >
        {props.nodeName}
      </span>
      <InfoIcon twinArrow={twinArrow} twinGraph={twinGraph} />
      <NodeNameAfterPlugin twinArrow={twinArrow} />
      <ArrowDiffBadges twinArrow={twinArrow} arrowDiff={arrowDiff} />
      {debugMode && <NodeDebugInfo twinArrow={twinArrow} />}
      {isHovered && <CopyToClipboard text={props.nodeName} className="ml-2" />}
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

function InfoIcon({
  twinArrow,
  twinGraph,
}: {
  twinArrow: TwinArrow;
  twinGraph: TwinGraph;
}) {
  let content = null;
  let messageL = null;
  let messageR = null;

  const badgeContent = getBadgeContent(twinGraph, twinArrow);

  if (badgeContent != null) {
    content = (
      <>
        <H2 text={badgeContent.header} />
        <p>{badgeContent.content}</p>
      </>
    );
  }

  if (twinArrow.l?.message != null) {
    const label = twinGraph.isDeltaGraph() ? " (Left Graph)" : "";
    messageL = (
      <>
        <H2 text={`Additional Information${label}`} />
        <p className="break-words">{twinArrow.l.message}</p>
      </>
    );
  }

  if (twinArrow.r?.message != null) {
    messageR = (
      <>
        <H2 text="Additional Information (Right Graph)" />
        <p className="break-words">{twinArrow.r.message}</p>
      </>
    );
  }

  if (content == null && messageL == null && messageR == null) {
    return null;
  }

  return (
    <UHoverCard
      content={
        <div className="flex flex-col gap-2">
          {content}
          {messageL}
          {messageR}
        </div>
      }
    >
      <BadgeInfo size={16} />
    </UHoverCard>
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

type ArrowDiff =
  | "node_became_reachable"
  | "node_became_unreachable"
  | "edge_became_excluded"
  | "edge_became_included"
  | "edge_was_removed"
  | "edge_was_added"
  | "excluded_edge_was_added"
  | "excluded_edge_was_removed"
  | "single_graph_unreachable"
  | "single_graph_edge_excluded"
  | "no_change";

function getArrowDiff(twinGraph: TwinGraph, twinArrow: TwinArrow): ArrowDiff {
  if (twinGraph.l != null) {
    const reachableL = twinGraph.l.isNodeReachable(twinArrow.points_to);
    const reachableR = twinGraph.r.isNodeReachable(twinArrow.points_to);

    if (!reachableL && reachableR) {
      return "node_became_reachable";
    } else if (reachableL && !reachableR) {
      return "node_became_unreachable";
    } else if (reachableL && reachableR) {
      if (twinArrow.l != null && twinArrow.r != null) {
        if (twinArrow.l.excluded && !twinArrow.r.excluded) {
          return "edge_became_included";
        } else if (!twinArrow.l.excluded && twinArrow.r.excluded) {
          return "edge_became_excluded";
        } else {
          return "no_change";
        }
      } else if (twinArrow.l == null && twinArrow.r != null) {
        if (twinArrow.r.excluded) {
          return "excluded_edge_was_added";
        } else {
          return "edge_was_added";
        }
      } else if (twinArrow.l != null && twinArrow.r == null) {
        if (twinArrow.l.excluded) {
          return "excluded_edge_was_removed";
        } else {
          return "edge_was_removed";
        }
      }
    }
  } else {
    const reachableR = twinGraph.r.isNodeReachable(twinArrow.points_to);
    if (!reachableR) {
      return "single_graph_unreachable";
    } else if (twinArrow.r?.excluded) {
      return "single_graph_edge_excluded";
    }
  }

  return "no_change";
}

function getPresenceColor(
  twinGraph: TwinGraph,
  twinArrow: TwinArrow,
): string | null {
  switch (getArrowDiff(twinGraph, twinArrow)) {
    case "node_became_reachable":
    case "edge_became_included":
    case "edge_was_added":
    case "excluded_edge_was_added":
      return "bg-added";
    case "node_became_unreachable":
    case "edge_became_excluded":
    case "edge_was_removed":
    case "excluded_edge_was_removed":
      return "bg-removed";
    case "single_graph_unreachable":
    case "single_graph_edge_excluded":
    case "no_change": {
      return null;
    }
  }
}

function getBadgeContent(
  twinGraph: TwinGraph,
  twinArrow: TwinArrow,
): { content: string; header: string } | null {
  switch (getArrowDiff(twinGraph, twinArrow)) {
    case "node_became_reachable":
    case "node_became_unreachable": {
      // these are covered by the "added" and "removed" badges
      return null;
    }
    case "edge_became_included": {
      return {
        content:
          "This edge exists in both graphs, it was excluded in the graph on the left but now it is included in the graph on the right.",
        header: "Edge was added to the graph",
      };
    }
    case "edge_was_added": {
      return {
        content:
          "This edge did not exist in the node on the left graph but it does exist in the node on the right.",
        header: "Edge was added to the node",
      };
    }

    case "excluded_edge_was_added": {
      return {
        content:
          "This edge was added to the node, but it was excluded from the graph.",
        header: "Excluded edge was added to the node",
      };
    }
    case "edge_became_excluded": {
      return {
        content:
          "This edge exists in both graphs, it was included in the graph on the left but now it is excluded from the graph on the right.",
        header: "Edge was removed from the graph",
      };
    }
    case "edge_was_removed": {
      return {
        content:
          "This edge existed in the node on the left graph but it does not exist in the node on the right. The node is still reachable though other edges.",
        header: "Edge was removed from the node",
      };
    }
    case "excluded_edge_was_removed": {
      return {
        content:
          "This edge existed on the graph on the left, but it wasn't followed. It was fully removed from the graph on the right.",
        header: "Excluded edge was removed from the node",
      };
    }
    case "single_graph_unreachable": {
      return {
        content:
          "This edge points to a node that is not reachable from the root node because all edges that lead to it are excluded.",
        header: "Node is not reachable",
      };
    }
    case "single_graph_edge_excluded": {
      return {
        content: `This edge was not followed during the graph traversal, but this node is still reachable through other edges in the graph. You can switch to "Reverse" mode (R keyboard shortcut) to see all edges that lead to this node.`,
        header: "This edge was not followed",
      };
    }
    case "no_change": {
      return null;
    }
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

  const min = Math.min(twinArrow.l?.skipped ?? 0, twinArrow.r?.skipped ?? 0);

  if (min > 0) {
    return (
      <UHoverCard
        asChild
        content={
          <SkippedNodesHovercardContent
            twinGraph={twinGraph}
            twinArrow={twinArrow}
            skipped={min}
          />
        }
      >
        <span
          className="bg-primary text-background text-xs py-0.5 px-3 me-1 rounded-lg cursor-pointer"
          onClick={onClick}
        >
          {`+${min}`}
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
  if (canExpand) {
    if (row.isCycle) {
      return <RefreshCw size={16} className="mx-2" />;
    }
    return row.expanded ? (
      <ChevronDown
        size={16}
        className="mx-2"
        onClick={() => {
          onToggleExpand(false);
        }}
      />
    ) : (
      <ChevronRight
        size={16}
        className="mx-2"
        onClick={() => {
          onToggleExpand(true);
        }}
      />
    );
  }
  return <span className="mx-2 w-4" />;
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

function NodeDebugInfo({ twinArrow }: { twinArrow: TwinArrow }) {
  return (
    <UDialog
      title="Debug Info"
      className="sm:max-w-3xl max-h-[80vh] overflow-hidden"
      trigger={
        // Stop propagation so opening the dialog doesn't also select the row.
        <Wrench
          size={16}
          className="cursor-pointer shrink-0"
          onClick={(e) => e.stopPropagation()}
        />
      }
    >
      <NodeDebugDialog twinArrow={twinArrow} />
    </UDialog>
  );
}
