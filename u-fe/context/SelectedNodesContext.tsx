// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
} from "react";
import type { NodeIDX } from "@/types";
import { get_selected_node_idxs } from "../../.build/wasm/unigraph_wasm";
import { useSimulationParams } from "./SimulationParamsContext";

/// Selected nodes are the ones that are selected from the visualization
/// view as a plain list of NodeIDXes. DO NOT confuse with focused node
/// (a single node row being focused/highlighted in the tree table)
export type SelectedNodesContextType = [
  NodeIDX[],
  (selectedNodex: NodeIDX[]) => void,
  () => void, // reset selection
];

const SelectedNodesContext = createContext<SelectedNodesContextType | null>(
  null,
);

export function SelectedNodesContextProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [selectedNodes, setSelectedNodes] = useState<NodeIDX[]>([]);
  const [simulationParams, setSimulationParams] = useSimulationParams();

  const resetSelectedNodes = useCallback(() => {
    setSelectedNodes([]);
    // if we're setting it to the empty selection we should also
    // reset the selection in simulation params.
    setSimulationParams({
      ...simulationParams,
      selection: {
        selection_from_point: { x: 0, y: 0 },
        selection_to_point: { x: 0, y: 0 },
        selection_type: "None",
      },
    });

    setTimeout(() => {
      // Nasty side effect.
      // This is a mutating getter that will return an empty selection
      // and it will also mark all nodes as unselected, which will reset the
      // simualation UI.
      // We need to call it after the whole state update settles and we have the
      // new (reset) selection on the backend side. After that getting the
      // selected node IDs will actually reste the selection in the simulation.
      // It's nasty because there was a perf issue and we only perform actual
      // selection/highlight at the end of the selection (as you mouse up) and
      // not on every frame change.
      get_selected_node_idxs();
    }, 10);
  }, [simulationParams, setSimulationParams]);

  const value: SelectedNodesContextType = useMemo(
    () => [selectedNodes, setSelectedNodes, resetSelectedNodes],
    [selectedNodes, resetSelectedNodes],
  );

  return (
    <SelectedNodesContext.Provider value={value}>
      {children}
    </SelectedNodesContext.Provider>
  );
}

export function useSelectedNodes(): SelectedNodesContextType {
  const context = useContext(SelectedNodesContext);

  if (context == null) {
    throw new Error(
      "useSelectedNodes must be used within a SelectedNodesContextProvider",
    );
  }
  return context;
}
