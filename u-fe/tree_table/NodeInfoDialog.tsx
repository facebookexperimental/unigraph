// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Fragment } from "react";
import type { Arrow } from "../__generated__/ts/Arrow";
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
      {twinGraph.isDeltaGraph() ? (
        <>
          <ArrowSection title="Edge (left)" arrow={twinArrow.l} />
          <ArrowSection title="Edge (right)" arrow={twinArrow.r} />
        </>
      ) : (
        <ArrowSection title="Edge" arrow={twinArrow.r} />
      )}
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

// ── Node / change / edge ────────────────────────────────────────

function NodeSection({
  twinGraph,
  nodeIDX,
}: {
  twinGraph: TwinGraph;
  nodeIDX: NodeIDX;
}) {
  const name = twinGraph.getNodeName(nodeIDX);
  const rows: Row[] = twinGraph.isDeltaGraph()
    ? [
        ["reachable", sideBySide(twinGraph, nodeIDX, reachableLabel)],
        ["tier", sideBySide(twinGraph, nodeIDX, tierLabel)],
      ]
    : [
        ["reachable", reachableLabel(twinGraph.r, nodeIDX)],
        ["tier", tierLabel(twinGraph.r, nodeIDX)],
      ];

  return (
    <Section title="Node">
      <div className="flex items-start gap-1">
        <span className="font-mono break-all">{displayNodeName(name)}</span>
        <CopyToClipboard text={name} />
      </div>
      <DefinitionList rows={rows} />
    </Section>
  );
}

/// `left → right` on one line, or a single value when both sides agree.
function sideBySide(
  twinGraph: TwinGraph,
  nodeIDX: NodeIDX,
  label: (graph: NativeGraph, nodeIDX: NodeIDX) => string,
): string {
  const left = twinGraph.l;
  if (left == null) {
    return label(twinGraph.r, nodeIDX);
  }
  const l = label(left, nodeIDX);
  const r = label(twinGraph.r, nodeIDX);
  return l === r ? l : `${l} → ${r}`;
}

/// The prose that used to live in the row's hovercard: why this row is
/// coloured the way it is, plus whatever the traversal had to say about the
/// edge on each side.
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

  const messages: Array<[string, string]> = [];
  if (twinArrow.l?.message != null) {
    messages.push(["Left", twinArrow.l.message]);
  }
  if (twinArrow.r?.message != null) {
    messages.push(["Right", twinArrow.r.message]);
  }

  if (explanation == null && flags.length === 0 && messages.length === 0) {
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
        <DefinitionList rows={[["node", flags.join(", ")]]} />
      )}
      {messages.map(([side, message]) => (
        <div key={side} className="flex flex-col gap-0.5">
          <div className="font-semibold">{`Traversal note (${side.toLowerCase()})`}</div>
          <p className="break-words text-foreground/80">{message}</p>
        </div>
      ))}
    </Section>
  );
}

/// The raw edge as it exists on one side of the graph. Note that an `Arrow`'s
/// own `points_from`/`points_to` are side-local indices, unlike the merged
/// indices on the enclosing `TwinArrow`.
function ArrowSection({
  title,
  arrow,
}: {
  title: string;
  arrow: Arrow | undefined;
}) {
  if (arrow == null) {
    return (
      <Section title={title}>
        <Empty text="edge does not exist on this side" />
      </Section>
    );
  }

  const rows: Row[] = [
    ["kind", edgeKind(arrow)],
    ["excluded", String(arrow.excluded)],
    ["skipped", String(arrow.skipped)],
  ];

  const dynamic = arrow.dynamic;
  if (dynamic != null) {
    rows.push(
      ["type_key", dynamic.type_key],
      ["edge_name", dynamic.edge_name],
      ["branch", dynamic.branch],
      ...Object.entries(dynamic.metadata ?? {}).map(
        ([key, value]): Row => [`metadata.${key}`, value],
      ),
    );
  }

  return (
    <Section title={title}>
      <DefinitionList rows={rows} />
    </Section>
  );
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
  const rows: Row[] = [
    ["merged IDX", String(twinArrow.points_to)],
    ["points_from (merged)", String(twinArrow.points_from)],
    ["node_diff bits", `0b${diff.toString(2).padStart(4, "0")}`],
  ];

  if (twinGraph.isDeltaGraph()) {
    rows.push(
      ["left edge (local)", localEdge(twinArrow.l)],
      ["right edge (local)", localEdge(twinArrow.r)],
    );
  }

  return (
    <Section title="Internals">
      <DefinitionList rows={rows} />
    </Section>
  );
}

function localEdge(arrow: Arrow | undefined): string {
  if (arrow == null) {
    return NONE;
  }
  return `${arrow.points_from} → ${arrow.points_to}`;
}

// ── Presentation ────────────────────────────────────────────────

type Row = [key: string, value: string];

function DefinitionList({ rows }: { rows: Row[] }) {
  if (rows.length === 0) {
    return <Empty text="none" />;
  }

  return (
    <dl className="grid grid-cols-[minmax(0,auto)_minmax(0,1fr)] gap-x-3 gap-y-0.5 font-mono">
      {rows.map(([key, value]) => (
        <Fragment key={key}>
          <dt className="text-foreground/60 break-all">{key}</dt>
          <dd className="break-all">{value}</dd>
        </Fragment>
      ))}
    </dl>
  );
}

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

function edgeKind(arrow: Arrow): string {
  if (arrow.tag != null) return `tagged (${arrow.tag})`;
  if (arrow.dynamic != null) return "dynamic";
  return "directed";
}

function reachableLabel(graph: NativeGraph, nodeIDX: NodeIDX): string {
  return String(graph.isNodeReachable(nodeIDX));
}

function tierLabel(graph: NativeGraph, nodeIDX: NodeIDX): string {
  const tier = graph.getNodeTierName(nodeIDX);
  if (tier == null) return NONE;
  const [name, idx] = tier;
  return `${name} (${idx})`;
}
