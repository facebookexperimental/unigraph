// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Shield, ShieldOff, X } from "lucide-react";
import { useMemo, useState } from "react";
import NodeNameInput from "../components/NodeNameInput";
import { Button } from "../components/ui/button";
import { Separator } from "../components/ui/separator";
import { useTreeTableRef } from "../context/GlobalElementRefs";
import {
  type MinCutProtectedEdge,
  type MinCutSink,
  useMinCut,
} from "../context/MinCutContext";
import { useNativeGraphs } from "../context/NativeGraphContext";
import { useSelectedPath } from "../context/SelectedPathContext";
import type NativeGraph from "../native/NativeGraph";
import { H1 } from "../Typography";
import type { MinCutResult } from "../__generated__/ts/MinCutResult";
import type { NodeIDX } from "../types";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

/**
 * Min Cut panel — pick a set of nodes (a feature to delete) and see the minimum
 * set of dependency edges to remove so that feature becomes unreachable from the
 * graph's entry points. The cut is computed over the whole selected set at once
 * (a single min-cut), not per node.
 *
 * Any proposed cut edge can be "protected" (shield icon), which moves it to the
 * exceptions section and reruns the algorithm to find an alternative cut that
 * routes around every protected edge.
 *
 * Layout: the computed cut and the protected exceptions sit at the top; the node
 * picker is pinned to the bottom (its typeahead opens upward). All state lives
 * in `MinCutContext` so it survives closing and reopening the panel.
 *
 * Single-graph only: the panel is not registered in comparison mode (see
 * `Explorer.tsx`), so `nativeGraphR` is always the sole graph here.
 */
export default function MinCutPanel() {
  const [, nativeGraphR] = useNativeGraphs();
  const {
    sinks,
    addSink,
    removeSink,
    protectedEdges,
    protectEdge,
    unprotectEdge,
    clear,
  } = useMinCut();
  const [inputValue, setInputValue] = useState("");
  const navigateToNode = useNavigateToNode();

  const result = useMemo(
    () =>
      sinks.length === 0
        ? null
        : nativeGraphR.minCut(
            sinks.map((s) => s.idx),
            protectedEdges,
          ),
    [nativeGraphR, sinks, protectedEdges],
  );

  const addSinkByName = (name: string) => {
    const idx = nativeGraphR.getNodeIDXByNameLog(name);
    setInputValue("");
    if (idx == null) return;
    addSink({ idx, name });
  };

  const hasState = sinks.length > 0 || protectedEdges.length > 0;

  return (
    <SidebarPanel storageKey="min-cut" defaultWidth={800}>
      <div className="flex flex-col h-full min-h-0">
        <div className="flex items-center justify-between">
          <H1 text="Min Cut" />
          {hasState && (
            <Button
              variant="ghost"
              size="sm"
              className="cursor-pointer"
              onClick={clear}
            >
              Clear
            </Button>
          )}
        </div>
        <Separator className="my-4" />
        <div className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-6">
          {result == null ? (
            <p className="text-xs text-muted-foreground">
              Add nodes below to compute the cut.
            </p>
          ) : (
            <CutResult
              result={result}
              nativeGraph={nativeGraphR}
              onNodeClick={navigateToNode}
              onProtect={protectEdge}
            />
          )}
          {protectedEdges.length > 0 && (
            <ProtectedEdges
              edges={protectedEdges}
              nativeGraph={nativeGraphR}
              onNodeClick={navigateToNode}
              onUnprotect={unprotectEdge}
            />
          )}
        </div>
        <div className="shrink-0 border-t pt-4 mt-4 flex flex-col gap-3">
          <SelectedSinks sinks={sinks} onRemove={removeSink} />
          <NodeNameInput
            value={inputValue}
            onChange={setInputValue}
            onSelect={addSinkByName}
            placeholder="Add a node to cut…"
            openUpward
          />
          <p className="text-xs text-muted-foreground">
            Pick the nodes to cut off. The minimum set of dependency edges to
            remove — so those nodes become unreachable dead code from the
            graph's entry points — is shown above. Shield an edge to protect it
            and find an alternative cut.
          </p>
        </div>
      </div>
    </SidebarPanel>
  );
}

// -- Implementation ---------------------------------------------------------

/// Replicates the footer node-search navigation: reveal the node in the tree
/// table (expand + scroll + highlight) and refocus the table for keyboard nav.
function useNavigateToNode(): (idx: NodeIDX) => void {
  const { setSelectedPath } = useSelectedPath();
  const treeTableRef = useTreeTableRef();
  return (idx: NodeIDX) => {
    setSelectedPath([idx], true);
    treeTableRef.current?.focus();
  };
}

function SelectedSinks({
  sinks,
  onRemove,
}: {
  sinks: MinCutSink[];
  onRemove: (idx: NodeIDX) => void;
}) {
  if (sinks.length === 0) {
    return <p className="text-xs text-muted-foreground">No nodes selected.</p>;
  }
  return (
    <div className="flex flex-col gap-1">
      {sinks.map((s) => (
        <div
          key={s.idx}
          className="flex items-center justify-between gap-2 rounded-md border px-2 py-1 text-xs"
        >
          <span className="truncate font-mono">{s.name}</span>
          <Button
            size="icon"
            variant="ghost"
            className="h-5 w-5 shrink-0 cursor-pointer"
            onClick={() => onRemove(s.idx)}
          >
            <X className="h-3 w-3" />
          </Button>
        </div>
      ))}
    </div>
  );
}

function CutResult({
  result,
  nativeGraph,
  onNodeClick,
  onProtect,
}: {
  result: MinCutResult;
  nativeGraph: NativeGraph;
  onNodeClick: (idx: NodeIDX) => void;
  onProtect: (edge: MinCutProtectedEdge) => void;
}) {
  return (
    <div>
      <SidebarPanelHeader text={`Edges to cut (${result.cut_edges.length})`} />
      {result.has_uncuttable_sink && (
        <p className="text-xs text-amber-600 dark:text-amber-500 mb-2">
          Some selected nodes are entry points and can't be cut off — you'd have
          to delete the module itself. The cut below covers the rest.
        </p>
      )}
      {result.cut_edges.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {result.blocked_by_protected
            ? "No cut avoids every protected edge — unprotect one to find a cut."
            : "Nothing to cut — the selected nodes are already unreachable."}
        </p>
      ) : (
        <div className="flex flex-col gap-1">
          {result.cut_edges.map((e) => (
            <EdgeRow
              key={`${e.from}-${e.to}`}
              from={e.from}
              to={e.to}
              nativeGraph={nativeGraph}
              onNodeClick={onNodeClick}
              action={
                <EdgeAction
                  tooltip="Protect this edge and find another cut"
                  onClick={() => onProtect({ from: e.from, to: e.to })}
                >
                  <Shield className="h-3 w-3" />
                </EdgeAction>
              }
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ProtectedEdges({
  edges,
  nativeGraph,
  onNodeClick,
  onUnprotect,
}: {
  edges: MinCutProtectedEdge[];
  nativeGraph: NativeGraph;
  onNodeClick: (idx: NodeIDX) => void;
  onUnprotect: (edge: MinCutProtectedEdge) => void;
}) {
  return (
    <div>
      <SidebarPanelHeader text={`Protected edges (${edges.length})`} />
      <p className="text-xs text-muted-foreground mb-2">
        These edges are never cut. The cut above routes around them.
      </p>
      <div className="flex flex-col gap-1">
        {edges.map((e) => (
          <EdgeRow
            key={`${e.from}-${e.to}`}
            from={e.from}
            to={e.to}
            nativeGraph={nativeGraph}
            onNodeClick={onNodeClick}
            action={
              <EdgeAction
                tooltip="Unprotect this edge"
                onClick={() => onUnprotect(e)}
              >
                <ShieldOff className="h-3 w-3" />
              </EdgeAction>
            }
          />
        ))}
      </div>
    </div>
  );
}

function EdgeRow({
  from,
  to,
  nativeGraph,
  onNodeClick,
  action,
}: {
  from: NodeIDX;
  to: NodeIDX;
  nativeGraph: NativeGraph;
  onNodeClick: (idx: NodeIDX) => void;
  action: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-2 rounded-md border px-2 py-1 text-xs font-mono">
      {action}
      <EdgeNode
        name={nativeGraph.getNodeName(from)}
        onClick={() => onNodeClick(from)}
      />
      <span className="text-muted-foreground shrink-0">→</span>
      <EdgeNode
        name={nativeGraph.getNodeName(to)}
        onClick={() => onNodeClick(to)}
      />
    </div>
  );
}

function EdgeAction({
  tooltip,
  onClick,
  children,
}: {
  tooltip: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Button
      size="icon"
      variant="ghost"
      className="h-5 w-5 shrink-0 cursor-pointer"
      title={tooltip}
      onClick={onClick}
    >
      {children}
    </Button>
  );
}

/// A clickable node name in an edge row. Clicking navigates the Explorer to it.
function EdgeNode({ name, onClick }: { name: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="truncate text-left hover:text-primary hover:underline cursor-pointer"
      title={name}
    >
      {name}
    </button>
  );
}
