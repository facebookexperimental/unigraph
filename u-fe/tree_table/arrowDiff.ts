// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import type TwinGraph from "../native/TwinGraph";

/// How a row's edge differs between the two sides of a delta graph.
///
/// One classification drives three things — the row's background colour, the
/// prose in its info dialog, and whether the name is struck through — so it
/// lives here rather than in the cell that first needed it.
export type ArrowDiff =
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

export function getArrowDiff(
  twinGraph: TwinGraph,
  twinArrow: TwinArrow,
): ArrowDiff {
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

export function getPresenceColor(arrowDiff: ArrowDiff): string | null {
  switch (arrowDiff) {
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

/// Prose for the info dialog. `null` where the row already says it — an added
/// or removed node carries a badge, so repeating it would be noise.
export function getArrowDiffExplanation(
  arrowDiff: ArrowDiff,
): { content: string; header: string } | null {
  switch (arrowDiff) {
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
