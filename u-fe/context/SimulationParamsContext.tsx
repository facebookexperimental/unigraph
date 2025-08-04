// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { SimulationParams } from "@/__generated__/ts/SimulationParams";
import { createContext, useContext, useMemo, useState } from "react";
import { get_simulation_params } from "../../.build/wasm/unigraph_wasm";

export type SimulationParamsContextType = [
  SimulationParams,
  (params: SimulationParams) => void,
];

const SimulationParamsContext =
  createContext<SimulationParamsContextType | null>(null);

export function SimulationParamsContextProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const defaultSimulationParams = useDefaultSimulationParams();
  const [simulationParams, setSimulationParams] = useState<SimulationParams>(
    defaultSimulationParams,
  );

  const value: SimulationParamsContextType = useMemo(
    () => [simulationParams, setSimulationParams],
    [simulationParams],
  );

  return (
    <SimulationParamsContext.Provider value={value}>
      {children}
    </SimulationParamsContext.Provider>
  );
}

export function useSimulationParams(): SimulationParamsContextType {
  const context = useContext(SimulationParamsContext);

  if (context == null) {
    throw new Error(
      "useSimulationParams must be used within a SimulationParamsContextProvider",
    );
  }
  return context;
}

function useDefaultSimulationParams(): SimulationParams {
  return useMemo(() => {
    // wasm sets some set of defaults for simulation params, we'll use those
    // as a starting point in JS
    const defaultSimulationParamsJSON = get_simulation_params();
    const defaultSimulationParams: SimulationParams = JSON.parse(
      defaultSimulationParamsJSON,
    );
    const simulationParams: SimulationParams = {
      ...defaultSimulationParams,
      colors: {
        node_main: [1.0, 0.1254902, 0.3372549],
        node_selected: [0.048, 0.0091, 0.4654],
        background: [0.04529412, 0.04137255, 0.04137255],
      },
    };
    return simulationParams;
  }, []);
}
