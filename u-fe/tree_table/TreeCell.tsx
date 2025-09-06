// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import {
  ArrowRight,
  BadgeInfo,
  ChevronDown,
  ChevronRight,
  RefreshCw,
} from "lucide-react";
import { H2 } from "../Typography";
import type { Arrow } from "../__generated__/ts/Arrow";
import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import UHoverCard from "../components/UHoverCard";
import { Badge } from "../components/ui/badge";
import { useTwinGraph } from "../context/NativeGraphContext";
import type TwinGraph from "../native/TwinGraph";
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

  const chevron = (() => {
    if (props.canExpand) {
      if (props.row.isCycle) {
        return <RefreshCw size={16} className="mx-2" />;
      }
      return props.row.expanded ? (
        <ChevronDown
          size={16}
          className="mx-2"
          onClick={() => {
            props.onToggleExpand(false);
          }}
        />
      ) : (
        <ChevronRight
          size={16}
          className="mx-2"
          onClick={() => {
            props.onToggleExpand(true);
          }}
        />
      );
    }
    return <span className="mx-2 w-4" />;
  })();

  const padding = [];
  const PaddingComponent = props.paddingComponent;
  for (let i = Math.max(props.minDepth - 1, 0); i < props.row.depth; i++) {
    padding.push(
      <PaddingComponent key={i} size={16} color="#333" className="mx-2" />,
    );
  }

  const color = getPresenceColor(twinGraph, twinArrow);

  let lineThrough = null;
  switch (getPresence(twinGraph, twinArrow)) {
    case "node_became_unreachable":
    case "single_graph_unreachable": {
      lineThrough = "text-foreground/50 line-through";
    }
  }

  return (
    <div className={clsx("flex items-center w-full h-full", color)}>
      {padding}
      {chevron}
      <ArrowBadge twinArrow={twinArrow} />
      <p
        className={clsx(
          "pe-4 text-ellipsis text-nowrap",
          isExcludedInBoth(twinGraph, twinArrow) && "text-foreground/50",
          lineThrough,
        )}
      >
        {props.nodeName}
      </p>
      <InfoIcon twinArrow={twinArrow} twinGraph={twinGraph} />
    </div>
  );
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
      return <EdgeBadge label={l.label} badgeType={l.t} />;
    } else {
      return (
        <span className="flex">
          <EdgeBadge label={l.label} badgeType={l.t} />
          <ArrowRight size={16} className="me-2" />
          <EdgeBadge label={r.label} badgeType={r.t} />
        </span>
      );
    }
  } else if (l != null) {
    return <EdgeBadge label={l.label} badgeType={l.t} />;
  } else if (r != null) {
    return <EdgeBadge label={r.label} badgeType={r.t} />;
  } else {
    return null;
  }
}

function EdgeBadge({
  label,
  badgeType,
}: { label: string; badgeType?: BadgeType }) {
  const color = badgeType === "tag" ? "bg-green-800" : "bg-orange-800";
  return (
    <Badge className={clsx("me-2 text-xs py-0 px-0.5", color)}>{label}</Badge>
  );
}

type BadgeType = "tag" | "dyn";
function getBadgeData(
  arrow: Arrow | null,
): { t: BadgeType; label: string } | null {
  if (arrow == null) return null;

  const tag = arrow.tag;
  const branch = arrow.branch;
  if (tag != null) {
    return { t: "tag", label: tag };
  } else if (branch != null) {
    const label = arrow.properties?.type ?? branch;
    return {
      t: "dyn",
      label,
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
    return twinArrow.l && twinArrow.l.excluded === true;
  }
}

function getPresence(
  twinGraph: TwinGraph,
  twinArrow: TwinArrow,
):
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
  | "no_change" {
  if (twinGraph.r != null) {
    const reachableL = twinGraph.l.isNodeReachable(twinArrow.points_to);
    const reachableR = twinGraph.r.isNodeReachable(twinArrow.points_to);

    if (reachableL && !reachableR) {
      return "node_became_reachable";
    } else if (!reachableL && reachableR) {
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
    const reachableL = twinGraph.l.isNodeReachable(twinArrow.points_to);
    if (!reachableL) {
      return "single_graph_unreachable";
    } else if (twinArrow.l?.excluded) {
      return "single_graph_edge_excluded";
    }
  }

  return "no_change";
}

function getPresenceColor(
  twinGraph: TwinGraph,
  twinArrow: TwinArrow,
): string | null {
  switch (getPresence(twinGraph, twinArrow)) {
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
  switch (getPresence(twinGraph, twinArrow)) {
    case "node_became_reachable": {
      return {
        content:
          "This node does not exist (or not reachable) in the graph on the left but it does exist in the graph on the right.",
        header: "Node was added to the graph",
      };
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
    case "node_became_unreachable": {
      return {
        content:
          "This node does not exist (or not reachable) in the graph on the right but it does exist in the graph on the left.",
        header: "Node was removed from the graph",
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
          "This edge existed in the node on the left graph but it does not exist in the node on the right.",
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
