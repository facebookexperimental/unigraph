// Copyright (c) Meta Platforms, Inc. and affiliates.

import { X } from "lucide-react";
import { useMemo, useState } from "react";
import NodeNameInput from "../components/NodeNameInput";
import { Button } from "../components/ui/button";
import { useNativeGraphs } from "../context/NativeGraphContext";
import type NativeGraph from "../native/NativeGraph";
import type { MinCutResult } from "../__generated__/ts/MinCutResult";
import type { NodeIDX } from "../types";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

/**
 * Min Cut panel — pick a set of nodes (a feature to delete) and see the minimum
 * set of dependency edges to remove so that feature becomes unreachable from the
 * graph's entry points. The cut is computed over the whole selected set at once
 * (a single min-cut), not per node.
 *
 * Single-graph only: the panel is not registered in comparison mode (see
 * `Explorer.tsx`), so `nativeGraphR` is always the sole graph here.
 */
export default function MinCutPanel() {
  const [, nativeGraphR] = useNativeGraphs();
  const [sinks, setSinks] = useState<Sink[]>([]);
  const [inputValue, setInputValue] = useState("");

  const result = useMemo(
    () =>
      sinks.length === 0 ? null : nativeGraphR.minCut(sinks.map((s) => s.idx)),
    [nativeGraphR, sinks],
  );

  const addSink = (name: string) => {
    const idx = nativeGraphR.getNodeIDXByNameLog(name);
    setInputValue("");
    if (idx == null) return;
    setSinks((prev) =>
      prev.some((s) => s.idx === idx) ? prev : [...prev, { idx, name }],
    );
  };

  const removeSink = (idx: NodeIDX) =>
    setSinks((prev) => prev.filter((s) => s.idx !== idx));

  return (
    <SidebarPanel width="w-[800px]">
      <SidebarPanelHeader text="Min Cut" />
      <p className="text-xs text-muted-foreground mb-4">
        Select the nodes you want to cut off. The minimum set of dependency
        edges to remove — so those nodes become unreachable dead code from the
        graph's entry points — is shown below.
      </p>
      <NodeNameInput
        value={inputValue}
        onChange={setInputValue}
        onSelect={addSink}
        placeholder="Add a node to cut…"
      />
      <div className="mt-4">
        <SelectedSinks sinks={sinks} onRemove={removeSink} />
      </div>
      {result != null && (
        <CutResult result={result} nativeGraph={nativeGraphR} />
      )}
    </SidebarPanel>
  );
}

// -- Implementation ---------------------------------------------------------

interface Sink {
  idx: NodeIDX;
  name: string;
}

function SelectedSinks({
  sinks,
  onRemove,
}: {
  sinks: Sink[];
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
}: {
  result: MinCutResult;
  nativeGraph: NativeGraph;
}) {
  return (
    <div className="mt-6">
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
            ? "No valid cut exists."
            : "Nothing to cut — the selected nodes are already unreachable."}
        </p>
      ) : (
        <div className="flex flex-col gap-1">
          {result.cut_edges.map((e) => (
            <div
              key={`${e.from}-${e.to}`}
              className="flex items-center gap-2 rounded-md border px-2 py-1 text-xs font-mono"
            >
              <span className="truncate">
                {nativeGraph.getNodeName(e.from)}
              </span>
              <span className="text-muted-foreground shrink-0">→</span>
              <span className="truncate">{nativeGraph.getNodeName(e.to)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
