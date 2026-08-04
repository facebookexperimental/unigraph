// Copyright (c) Meta Platforms, Inc. and affiliates.

import { ChevronDown, ChevronRight } from "lucide-react";
import { Fragment, useState } from "react";
import type { Arrow } from "../__generated__/ts/Arrow";
import type { GraphNode } from "../__generated__/ts/GraphNode";
import type { TwinArrow } from "../__generated__/ts/TwinArrow";
import CopyToClipboard from "../components/CopyToClipboard";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "../components/ui/collapsible";
import { useTwinGraph } from "../context/NativeGraphContext";
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

const NONE = "—";

/// Everything we know about the node a row points to, plus the raw edge(s)
/// that lead to it. Only rendered while the dialog is open — the WASM lookups
/// here are unbatched and rebuild the node's whole edge list.
export default function NodeDebugDialog({
  twinArrow,
}: {
  twinArrow: TwinArrow;
}) {
  const twinGraph = useTwinGraph();
  const nodeIDX = twinArrow.points_to;

  return (
    // `min-h-0` so this shrinks inside the dialog's grid and actually scrolls
    // instead of overflowing it.
    <div className="flex flex-col gap-3 text-xs overflow-auto min-h-0">
      <NodeSection twinGraph={twinGraph} nodeIDX={nodeIDX} />
      <DiffSection twinGraph={twinGraph} twinArrow={twinArrow} />
      {twinGraph.isDeltaGraph() ? (
        <>
          <ArrowSection title="Edge (left)" arrow={twinArrow.l} />
          <ArrowSection title="Edge (right)" arrow={twinArrow.r} />
        </>
      ) : (
        <ArrowSection title="Edge" arrow={twinArrow.r} />
      )}
      {twinGraph.l != null && (
        <NodeDetailsSections
          graph={twinGraph.l}
          nodeIDX={nodeIDX}
          suffix=" (left)"
        />
      )}
      <NodeDetailsSections
        graph={twinGraph.r}
        nodeIDX={nodeIDX}
        suffix={twinGraph.isDeltaGraph() ? " (right)" : ""}
      />
    </div>
  );
}

function NodeSection({
  twinGraph,
  nodeIDX,
}: {
  twinGraph: TwinGraph;
  nodeIDX: NodeIDX;
}) {
  const name = twinGraph.getNodeName(nodeIDX);
  const rows: Row[] = [
    ["merged IDX", String(nodeIDX)],
    ["reachable (right)", String(twinGraph.r.isNodeReachable(nodeIDX))],
    ["tier (right)", tierLabel(twinGraph.r, nodeIDX)],
  ];

  if (twinGraph.l != null) {
    rows.push(
      ["reachable (left)", String(twinGraph.l.isNodeReachable(nodeIDX))],
      ["tier (left)", tierLabel(twinGraph.l, nodeIDX)],
    );
  }

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

function DiffSection({
  twinGraph,
  twinArrow,
}: {
  twinGraph: TwinGraph;
  twinArrow: TwinArrow;
}) {
  if (!twinGraph.isDeltaGraph()) {
    return null;
  }

  const diff = twinArrow.node_diff;
  return (
    <Section title="Node diff">
      <DefinitionList
        rows={[
          ["raw bits", `0b${diff.toString(2).padStart(4, "0")}`],
          ["missing in left", String(nodeDoesNotExistInL(diff))],
          ["missing in right", String(nodeDoesNotExistInR(diff))],
          ["edges changed", String(nodeEdgesChanged(diff))],
          ["metrics changed", String(nodeMetricsChanged(diff))],
        ]}
      />
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
    ["points_from (local IDX)", String(arrow.points_from)],
    ["points_to (local IDX)", String(arrow.points_to)],
    ["excluded", String(arrow.excluded)],
    ["skipped", String(arrow.skipped)],
    ["message", arrow.message ?? NONE],
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

function NodeDetailsSections({
  graph,
  nodeIDX,
  suffix,
}: {
  graph: NativeGraph;
  nodeIDX: NodeIDX;
  suffix: string;
}) {
  const node = graph.getMapNode(nodeIDX);

  return (
    <>
      <Section title={`Metrics${suffix}`}>
        <DefinitionList
          rows={Object.entries(node.metrics ?? {}).map(
            ([key, value]): Row => [key, String(value)],
          )}
        />
      </Section>
      <Section title={`Properties${suffix}`}>
        <DefinitionList rows={Object.entries(node.properties ?? {})} />
      </Section>
      <Section title={`Labels${suffix}`}>
        <DefinitionList
          rows={Object.entries(node.labels ?? {}).map(
            ([key, values]): Row => [key, values.join(", ")],
          )}
        />
      </Section>
      <EdgesSection node={node} suffix={suffix} />
    </>
  );
}

/// Collapsed by default — a node can have thousands of edges, and pretty
/// printing them is only worth it when someone actually asks.
function EdgesSection({ node, suffix }: { node: GraphNode; suffix: string }) {
  const [open, setOpen] = useState(false);
  const edges = {
    edges_directed: node.edges_directed,
    edges_tagged: node.edges_tagged,
    edges_dynamic: node.edges_dynamic,
  };
  const count = countEdges(node);

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="w-full">
        <div className="flex items-center gap-1 text-[10px] font-semibold uppercase tracking-wider text-foreground/50 border-b border-border pb-0.5">
          {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          {`Edges${suffix} (${count})`}
        </div>
      </CollapsibleTrigger>
      <CollapsibleContent>
        {count === 0 ? (
          <Empty text="none" />
        ) : (
          <Pre
            text={JSON.stringify(edges, null, 2)}
            className="mt-1 max-h-80 text-[11px] leading-tight"
          />
        )}
      </CollapsibleContent>
    </Collapsible>
  );
}

function countEdges(node: GraphNode): number {
  const directed = node.edges_directed?.length ?? 0;
  const tagged = Object.values(node.edges_tagged ?? {}).reduce(
    (sum, targets) => sum + targets.length,
    0,
  );
  const dynamic = Object.values(node.edges_dynamic ?? {})
    .flatMap((byName) => Object.values(byName))
    .flatMap((edge) => Object.values(edge.branches))
    .reduce((sum, targets) => sum + targets.length, 0);

  return directed + tagged + dynamic;
}

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
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-foreground/50 border-b border-border pb-0.5">
        {title}
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

function tierLabel(graph: NativeGraph, nodeIDX: NodeIDX): string {
  const tier = graph.getNodeTierName(nodeIDX);
  if (tier == null) return NONE;
  const [name, idx] = tier;
  return `${name} (${idx})`;
}
