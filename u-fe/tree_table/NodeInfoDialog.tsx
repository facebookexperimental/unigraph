// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { Arrow } from "../__generated__/ts/Arrow";
import type { DynamicEdgeInfo } from "../__generated__/ts/DynamicEdgeInfo";
import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import CopyToClipboard from "../components/CopyToClipboard";
import { useDebugMode } from "../context/DebugModeContext";
import { useTwinGraph } from "../context/NativeGraphContext";
import { formatJson } from "../json_diff/JsonDiffModel";
import UJSONDiff from "../json_diff/UJSONDiff";
import { displayNodeName } from "../lib/utils";
import type NativeGraph from "../native/NativeGraph";
import {
  nodeDoesNotExistInL,
  nodeDoesNotExistInR,
  nodeEdgesChanged,
  nodeMetricsChanged,
} from "../native/NodeDiff";
import type TwinGraph from "../native/TwinGraph";
import type { NodeIDX } from "../types";
import { Pre } from "../Typography";
import type { CompareRow } from "./CompareTable";
import { Absent, CompareTable, textRow } from "./CompareTable";
import { TierBadge } from "./columns/tiers";
import type { ArrowDiff } from "./arrowDiff";
import { getArrowDiff, getArrowDiffExplanation } from "./arrowDiff";

const NONE = "—";

/// Everything known about the node a row points to: why it is highlighted, the
/// edge that leads to it on each side, and its full `MapGraph` form.
///
/// In delta mode the node's JSON is rendered as a unified diff rather than two
/// panes — a `GraphNode` is mostly edges, nearly all of which are identical
/// between two versions of a graph, so two panes would leave the reader doing
/// the comparison by eye.
///
/// Only rendered while the dialog is open. `getMapNode` is unbatched and
/// rebuilds the node's whole edge list, so this must not run per row.
export default function NodeInfoDialog({
  twinArrow,
}: {
  twinArrow: TwinArrow;
}) {
  const twinGraph = useTwinGraph();
  const [debugMode] = useDebugMode();
  const nodeIDX = twinArrow.points_to;
  const arrowDiff = getArrowDiff(twinGraph, twinArrow);

  return (
    // `min-h-0` so this shrinks inside the dialog's grid and actually scrolls
    // instead of overflowing it.
    <div className="flex flex-col gap-3 text-xs overflow-auto min-h-0">
      <NodeSection twinGraph={twinGraph} nodeIDX={nodeIDX} />
      <WhatChangedSection twinArrow={twinArrow} arrowDiff={arrowDiff} />
      <EdgeSection twinGraph={twinGraph} twinArrow={twinArrow} />
      <NodeDataSection
        twinGraph={twinGraph}
        nodeIDX={nodeIDX}
        arrowDiff={arrowDiff}
      />
      {debugMode && (
        <InternalsSection twinGraph={twinGraph} twinArrow={twinArrow} />
      )}
    </div>
  );
}

// ── Node data ───────────────────────────────────────────────────

/// The node **as authored** — its `MapGraph` form, which is what the two
/// graphs actually store. In delta mode that is a structural JSON diff rather
/// than two panes of raw text: a `GraphNode` is mostly edges, nearly all of
/// which are identical between two versions of a graph, so two panes leave the
/// reader doing the comparison by eye. `UJSONDiff` renders both sides into one
/// scroll container, so they cannot drift apart.
function NodeDataSection({
  twinGraph,
  nodeIDX,
  arrowDiff,
}: {
  twinGraph: TwinGraph;
  nodeIDX: NodeIDX;
  arrowDiff: ArrowDiff;
}) {
  const left = twinGraph.l;

  if (left == null) {
    return (
      <Section title="Node data">
        <Pre
          text={formatJson(twinGraph.r.getMapNode(nodeIDX))}
          className="max-h-[55vh] text-[11px] leading-tight"
        />
      </Section>
    );
  }

  return (
    <Section title="Node data (as authored)">
      {/* UJSONDiff virtualizes against its scroll container, so it needs a
          bounded height rather than growing with the content. */}
      <div className="h-[55vh] min-h-80">
        <UJSONDiff
          left={left.getMapNode(nodeIDX)}
          right={twinGraph.r.getMapNode(nodeIDX)}
          leftLabel="Left (before)"
          rightLabel="Right (after)"
          identicalNote={
            sidesDiffer(twinGraph, nodeIDX, arrowDiff) ? (
              <TraversalOnlyNote />
            ) : undefined
          }
        />
      </div>
    </Section>
  );
}

/// A row can be painted as changed while the node itself is untouched: the two
/// graphs can hold the same nodes and edges and differ only in their traversal
/// configs, so a different branch gets followed. Saying "identical" with no
/// explanation next to a green or red row reads like a bug.
function TraversalOnlyNote() {
  return (
    <p className="text-muted-foreground text-xs">
      This row is marked as changed, but the node is defined the same way in
      both graphs — same metrics, same edges. What differs is which of those
      edges the traversal followed. Check the edge sections above, and the
      traversal config diff.
    </p>
  );
}

/// Whether the twin reports any difference for this node, whatever its data
/// says. Reachability and tier come from the traversal, not from the node.
function sidesDiffer(
  twinGraph: TwinGraph,
  nodeIDX: NodeIDX,
  arrowDiff: ArrowDiff,
): boolean {
  const left = twinGraph.l;
  if (left == null) return false;

  return (
    arrowDiff !== "no_change" ||
    left.isNodeReachable(nodeIDX) !== twinGraph.r.isNodeReachable(nodeIDX) ||
    tierLabel(left, nodeIDX) !== tierLabel(twinGraph.r, nodeIDX)
  );
}

// ── Node / state / edge ─────────────────────────────────────────

function NodeSection({
  twinGraph,
  nodeIDX,
}: {
  twinGraph: TwinGraph;
  nodeIDX: NodeIDX;
}) {
  const name = twinGraph.getNodeName(nodeIDX);
  const left = twinGraph.l;
  const right = twinGraph.r;

  const leftTier = left?.getNodeTierName(nodeIDX) ?? null;
  const rightTier = right.getNodeTierName(nodeIDX) ?? null;

  const rows: CompareRow[] = [
    textRow(
      "reachable",
      left == null ? null : String(left.isNodeReachable(nodeIDX)),
      String(right.isNodeReachable(nodeIDX)),
    ),
    {
      label: "tier",
      left: leftTier == null ? <Absent /> : <TierBadge tier={leftTier} />,
      right: rightTier == null ? <Absent /> : <TierBadge tier={rightTier} />,
      changed: leftTier?.[1] !== rightTier?.[1],
    },
  ];

  return (
    <Section title="Node">
      <div className="flex items-start gap-1">
        <span className="font-mono break-all">{displayNodeName(name)}</span>
        <CopyToClipboard text={name} />
      </div>
      <CompareTable rows={rows} isDelta={twinGraph.isDeltaGraph()} />
    </Section>
  );
}

/// Why this row is coloured the way it is. One statement about the row, so it
/// spans both columns rather than sitting in the comparison.
function WhatChangedSection({
  twinArrow,
  arrowDiff,
}: {
  twinArrow: TwinArrow;
  arrowDiff: ArrowDiff;
}) {
  const explanation = getArrowDiffExplanation(arrowDiff);
  const diff = twinArrow.node_diff;
  const flags = [
    nodeDoesNotExistInL(diff) && "added",
    nodeDoesNotExistInR(diff) && "removed",
    nodeEdgesChanged(diff) && "edges changed",
    nodeMetricsChanged(diff) && "metrics changed",
  ].filter((flag): flag is string => flag !== false);

  if (explanation == null && flags.length === 0) {
    return null;
  }

  return (
    <Section title="What changed">
      {explanation != null && (
        <div className="flex flex-col gap-0.5">
          <div className="font-semibold">{explanation.header}</div>
          <p className="text-foreground/80">{explanation.content}</p>
        </div>
      )}
      {flags.length > 0 && (
        <div className="flex gap-1 font-mono">
          {flags.map((flag) => (
            <span
              key={flag}
              className="border-accent-foreground/40 bg-accent rounded border px-1.5"
            >
              {flag}
            </span>
          ))}
        </div>
      )}
    </Section>
  );
}

/// The edge leading to this node, both sides on one row per field.
///
/// An `Arrow`'s own `points_from`/`points_to` are side-local indices, unlike
/// the merged indices on the enclosing `TwinArrow` — those live under
/// Internals, behind debug mode.
function EdgeSection({
  twinGraph,
  twinArrow,
}: {
  twinGraph: TwinGraph;
  twinArrow: TwinArrow;
}) {
  const isDelta = twinGraph.isDeltaGraph();
  const l = twinArrow.l;
  const r = twinArrow.r;

  if (l == null && r == null) {
    return (
      <Section title="Edge">
        <Empty text="this row is not reached via an edge" />
      </Section>
    );
  }

  const rows: CompareRow[] = [
    textRow("kind", edgeKind(l), edgeKind(r)),
    textRow(
      "excluded",
      boolField(l, (a) => a.excluded),
      boolField(r, (a) => a.excluded),
    ),
    textRow(
      "skipped",
      numField(l, (a) => a.skipped),
      numField(r, (a) => a.skipped),
    ),
  ];

  for (const key of dynamicKeys(l?.dynamic, r?.dynamic)) {
    rows.push(
      textRow(
        key,
        dynamicField(l?.dynamic, key),
        dynamicField(r?.dynamic, key),
      ),
    );
  }

  // The traversal's own explanation of why it did or did not follow this edge.
  // Long prose, and the two sides usually differ only in one word, so it reads
  // far better on one row than stacked.
  if (l?.message != null || r?.message != null) {
    rows.push(textRow("message", l?.message ?? null, r?.message ?? null));
  }

  return (
    <Section title={isDelta ? "Edge (left → right)" : "Edge"}>
      {isDelta && (l == null || r == null) && (
        <p className="text-muted-foreground">
          {`This edge exists only in the ${l == null ? "right" : "left"} graph.`}
        </p>
      )}
      <CompareTable rows={rows} isDelta={isDelta} />
    </Section>
  );
}

function edgeKind(arrow: Arrow | undefined): string | null {
  if (arrow == null) return null;
  if (arrow.tag != null) return `tagged (${arrow.tag})`;
  if (arrow.dynamic != null) return "dynamic";
  return "directed";
}

function boolField(
  arrow: Arrow | undefined,
  read: (arrow: Arrow) => boolean,
): string | null {
  return arrow == null ? null : String(read(arrow));
}

function numField(
  arrow: Arrow | undefined,
  read: (arrow: Arrow) => number,
): string | null {
  return arrow == null ? null : String(read(arrow));
}

/// Union of the dynamic-edge fields present on either side, so a field that
/// appears on only one of them still gets a row.
function dynamicKeys(
  left: DynamicEdgeInfo | undefined,
  right: DynamicEdgeInfo | undefined,
): string[] {
  if (left == null && right == null) return [];
  const metadata = new Set([
    ...Object.keys(left?.metadata ?? {}),
    ...Object.keys(right?.metadata ?? {}),
  ]);
  return [
    "type_key",
    "edge_name",
    "branch",
    ...[...metadata].sort().map((key) => `metadata.${key}`),
  ];
}

function dynamicField(
  dynamic: DynamicEdgeInfo | undefined,
  key: string,
): string | null {
  if (dynamic == null) return null;
  if (key.startsWith("metadata.")) {
    return dynamic.metadata?.[key.slice("metadata.".length)] ?? null;
  }
  if (key === "type_key") return dynamic.type_key;
  if (key === "edge_name") return dynamic.edge_name;
  if (key === "branch") return dynamic.branch;
  return null;
}

/// Index-space plumbing. Useful when something looks wrong with the merge
/// itself, noise otherwise — so it stays behind the debug-mode toggle.
function InternalsSection({
  twinGraph,
  twinArrow,
}: {
  twinGraph: TwinGraph;
  twinArrow: TwinArrow;
}) {
  const diff = twinArrow.node_diff;
  const rows: CompareRow[] = [
    {
      label: "merged IDX",
      left: String(twinArrow.points_to),
      right: String(twinArrow.points_to),
    },
    {
      label: "points_from (merged)",
      left: String(twinArrow.points_from),
      right: String(twinArrow.points_from),
    },
    {
      label: "node_diff bits",
      left: `0b${diff.toString(2).padStart(4, "0")}`,
      right: `0b${diff.toString(2).padStart(4, "0")}`,
    },
    textRow(
      "edge (side-local)",
      localEdge(twinArrow.l),
      localEdge(twinArrow.r),
    ),
  ];

  return (
    <Section title="Internals">
      <CompareTable rows={rows} isDelta={twinGraph.isDeltaGraph()} />
    </Section>
  );
}

function localEdge(arrow: Arrow | undefined): string | null {
  return arrow == null ? null : `${arrow.points_from} → ${arrow.points_to}`;
}

// ── Presentation ────────────────────────────────────────────────

function Section({
  title,
  action,
  children,
}: {
  title: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between gap-2 text-[10px] font-semibold uppercase tracking-wider text-foreground/50 border-b border-border pb-0.5">
        <span>{title}</span>
        {action}
      </div>
      {children}
    </div>
  );
}

function Empty({ text }: { text: string }) {
  return <div className="text-foreground/40 italic">{text}</div>;
}

function tierLabel(graph: NativeGraph, nodeIDX: NodeIDX): string {
  const tier = graph.getNodeTierName(nodeIDX);
  if (tier == null) return NONE;
  const [name, idx] = tier;
  return `${name} (${idx})`;
}
