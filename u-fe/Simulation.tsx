// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Play, Settings2, Split } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  get_selected_node_idxs,
  set_event_loop_active,
  set_simulation_params,
} from "../.build/wasm/unigraph_wasm.js";
import { IS_DEBUG_MODE } from "./DebugMode.js";
import { H2 } from "./Typography.js";
import type { SelectionType } from "./__generated__/ts/SelectionType.js";
import type { TsVec2 } from "./__generated__/ts/TsVec2.js";
import ErrorBoundary from "./components/ErrorBoundary.js";
import UButton from "./components/UButton.js";
import UToggleButton from "./components/UToggleButton.js";
import { Button } from "./components/ui/button.js";
import { Label } from "./components/ui/label";
import { Separator } from "./components/ui/separator.js";
import { Slider } from "./components/ui/slider";
import { Toggle } from "./components/ui/toggle";
import { useNativeGraph } from "./context/NativeGraphContext.js";
import { useSelectedNodes } from "./context/SelectedNodesContext.js";
import { useSimulationParams } from "./context/SimulationParamsContext.js";
import formatNumber from "./lib/formatNumber.js";

const HIDE_IF_TOO_MANY_NODES_THRESHOLD = 50000;

export default function Simulation() {
  const nativeGraph = useNativeGraph();
  const [paramsVisible, setParamsVisible] = useState(false);

  const reachableCount =
    nativeGraph.stats().num_all_nodes -
    nativeGraph.stats().num_unreachable_nodes;

  const isTooMany = reachableCount > HIDE_IF_TOO_MANY_NODES_THRESHOLD;

  const [bypassTooMany, setBypassTooMany] = useState(false);

  return (
    <div className="flex h-full border-r">
      {paramsVisible && <ParamsPanel />}

      <div className="flex w-[600px] grow-1 shrink-0 relative">
        {isTooMany && !bypassTooMany ? (
          <TooManyNodesDialog
            setBypassTooMany={setBypassTooMany}
            reachableNodesCount={reachableCount}
          />
        ) : (
          <SimulationImpl
            setParamsVisible={setParamsVisible}
            paramsVisible={paramsVisible}
          />
        )}
      </div>
    </div>
  );
}

function TooManyNodesDialog({
  setBypassTooMany,
  reachableNodesCount,
}: {
  setBypassTooMany: (hide: boolean) => void;
  reachableNodesCount: number;
}) {
  return (
    <div className="flex flex-col gap-2 items-center justify-center h-full p-4 bg-card text-center">
      <H2 text="Too Many Nodes to Visualize" />
      <p>
        The graph has too many nodes. The visualization may become very slow and
        is hidden by default.
      </p>
      <p className="text-muted-foreground">
        {formatNumber(reachableNodesCount)} reachable nodes found.
      </p>
      <UButton
        variant="default"
        className="mt-4"
        onClick={() => {
          setBypassTooMany(true);
        }}
      >
        Enable Anyway
      </UButton>
    </div>
  );
}

function SimulationImpl({
  setParamsVisible,
  paramsVisible,
}: { setParamsVisible: (visible: boolean) => void; paramsVisible: boolean }) {
  const [_selectedNodes, setSelectedNodes] = useSelectedNodes();

  const [simulationParams, setSimulationParams] = useSimulationParams();

  useEffect(() => {
    set_simulation_params(JSON.stringify(simulationParams));
  }, [simulationParams]);

  useEffect(() => {
    // Start running the event loop and all wgpu stuff when
    // this component mounts
    set_event_loop_active(true);
    // When the component unmounts we can stop the loop and let it
    // chill.
    return () => set_event_loop_active(false);
  }, []);

  const onCanvasClick = useCallback(
    (_point: TsVec2) => {
      if (simulationParams == null) {
        return;
      }
      setSimulationParams({
        ...simulationParams,
        selection: {
          ...simulationParams.selection,
          selection_type: "None",
        },
      });
    },
    [simulationParams, setSimulationParams],
  );

  const onCanvasSelect = useCallback(
    (from: TsVec2, to: TsVec2) => {
      if (simulationParams == null) {
        return;
      }
      const selection_type: SelectionType = "Box";
      const selection = {
        selection_from_point: from,
        selection_to_point: to,
        selection_type,
      };
      setSimulationParams({
        ...simulationParams,
        // stop simulation when selecting. Otherwise kinda hard
        // to hunt down the moving nodes
        active: false,
        selection,
      });
    },
    [simulationParams, setSimulationParams],
  );

  const onCanvasSelectComplete = useCallback(
    (from: TsVec2, to: TsVec2) => {
      if (simulationParams == null) {
        return;
      }
      const selection_type: SelectionType = "Box";
      const selection = {
        selection_from_point: from,
        selection_to_point: to,
        selection_type,
      };
      setSimulationParams({
        ...simulationParams,
        active: true,
        selection,
      });
      const selectedNodeIDXs = get_selected_node_idxs();
      setSelectedNodes(Array.from(selectedNodeIDXs));
    },
    [simulationParams, setSelectedNodes, setSimulationParams],
  );

  return (
    <ErrorBoundary>
      <Canvas
        onClick={onCanvasClick}
        onSelect={onCanvasSelect}
        onSelectComplete={onCanvasSelectComplete}
      />
      <SimulationParamsToggle
        selected={paramsVisible}
        onSelectedChange={setParamsVisible}
      />
    </ErrorBoundary>
  );
}

function SimulationParamsToggle({
  selected,
  onSelectedChange,
}: { selected?: boolean; onSelectedChange?: (selected: boolean) => void }) {
  return (
    <Button
      size="icon"
      className="cursor-pointer absolute top-2 left-2 z-10 mt-2"
      variant={selected ? "default" : "secondary"}
      onClick={() => {
        if (onSelectedChange) {
          onSelectedChange(!selected);
        }
      }}
    >
      <Settings2 />
    </Button>
  );
}

function ParamsPanel() {
  const [simulationParams, setSimulationParams] = useSimulationParams();

  return (
    <div className="px-6 py-4 flex flex-col gap-4 w-52 bg-card">
      <div className="flex gap-4">
        <Toggle
          pressed={simulationParams.active}
          onPressedChange={(active) => {
            setSimulationParams({
              ...simulationParams,
              active,
            });
          }}
        >
          <Play />
        </Toggle>
        <Toggle
          pressed={simulationParams.render_edges}
          onPressedChange={(render_edges) => {
            setSimulationParams({
              ...simulationParams,
              render_edges,
            });
          }}
        >
          <Split />
        </Toggle>
      </div>
      <div className="flex flex-col gap-4 grow-1">
        <UToggleButton
          className="w-full"
          selected={!simulationParams.disable_gravity}
          onSelectedChange={(disable_gravity) => {
            setSimulationParams({
              ...simulationParams,
              disable_gravity: !disable_gravity,
            });
          }}
        >
          {`Antigravity (${formatNumber(simulationParams.gravity_force_a)})`}
        </UToggleButton>

        <SimulationSlider
          value={simulationParams.gravity_force_a}
          min={0.0}
          max={200.0}
          precision={0}
          scale="logarithmic"
          onChange={(gravity_force_a) => {
            setSimulationParams({
              ...simulationParams,
              gravity_force_a,
            });
          }}
        />

        <UToggleButton
          className="w-full"
          selected={!simulationParams.disable_edge_forces}
          onSelectedChange={(disable_edge_forces) => {
            setSimulationParams({
              ...simulationParams,
              disable_edge_forces: !disable_edge_forces,
            });
          }}
        >
          {`Edge Forces (${formatNumber(simulationParams.edge_force_a)})`}
        </UToggleButton>

        <SimulationSlider
          value={simulationParams.edge_force_a}
          min={0}
          max={300}
          precision={4}
          scale="logarithmic"
          onChange={(edge_force_a) => {
            setSimulationParams({
              ...simulationParams,
              edge_force_a,
            });
          }}
        />

        {IS_DEBUG_MODE && (
          <SimulationSlider
            label="ln(1 + len * x)"
            value={simulationParams.edge_force_b}
            min={0.0}
            max={10.0}
            precision={2}
            scale="linear"
            onChange={(edge_force_b) => {
              setSimulationParams({
                ...simulationParams,
                edge_force_b,
              });
            }}
          />
        )}

        <UToggleButton
          className="w-full"
          selected={!simulationParams.disable_center_pull}
          onSelectedChange={(disable_center_pull) => {
            setSimulationParams({
              ...simulationParams,
              disable_center_pull: !disable_center_pull,
            });
          }}
        >
          {`Center Pull (${formatNumber(simulationParams.center_pull_force_multiplier)})`}
        </UToggleButton>

        <SimulationSlider
          value={simulationParams.center_pull_force_multiplier}
          min={0.0}
          max={100.0}
          precision={2}
          scale="logarithmic"
          onChange={(center_pull_force_multiplier) => {
            setSimulationParams({
              ...simulationParams,
              center_pull_force_multiplier,
            });
          }}
        />

        <Separator />

        {IS_DEBUG_MODE && (
          <SimulationSlider
            label="Total Force multiplier"
            value={simulationParams.total_force_multiplier}
            min={0.001}
            max={100.0}
            precision={2}
            scale="logarithmic"
            onChange={(total_force_multiplier) => {
              setSimulationParams({
                ...simulationParams,
                total_force_multiplier,
              });
            }}
          />
        )}

        <SimulationSlider
          label="Max Velocity"
          value={simulationParams.max_velocity_multiplier}
          min={0.0001}
          max={0.3}
          precision={2}
          scale="logarithmic"
          onChange={(max_velocity_multiplier) => {
            setSimulationParams({
              ...simulationParams,
              max_velocity_multiplier,
            });
          }}
        />

        <SimulationSlider
          label="Slowdown"
          value={simulationParams.slowdown}
          min={0.01}
          max={1.0}
          precision={2}
          scale="logarithmic"
          onChange={(slowdown) => {
            setSimulationParams({
              ...simulationParams,
              slowdown,
            });
          }}
        />

        {IS_DEBUG_MODE && (
          <SimulationSlider
            label="Frames / Compute"
            value={simulationParams.compute_forces_every_n_frames}
            min={1}
            max={10}
            precision={0}
            onChange={(compute_forces_every_n_frames) => {
              setSimulationParams({
                ...simulationParams,
                compute_forces_every_n_frames: Math.floor(
                  compute_forces_every_n_frames,
                ),
              });
            }}
          />
        )}
      </div>
    </div>
  );
}

function Canvas(props: {
  onClick: (point: TsVec2) => void;
  onSelect: (from: TsVec2, to: TsVec2) => void;
  onSelectComplete: (from: TsVec2, to: TsVec2) => void;
}) {
  const [onMouseDownPoint, setOnMouseDownPoint] = useState<TsVec2 | null>(null);

  return (
    <canvas
      id="canvas"
      className="h-full w-full"
      onMouseDown={(e: React.MouseEvent<HTMLCanvasElement, MouseEvent>) => {
        setOnMouseDownPoint(getClickPoint(e));
      }}
      onMouseUp={(e: React.MouseEvent<HTMLCanvasElement, MouseEvent>) => {
        const point = getClickPoint(e);
        if (onMouseDownPoint != null) {
          if (
            onMouseDownPoint.x === point.x &&
            onMouseDownPoint.y === point.y
          ) {
            props.onClick(point);
          } else {
            props.onSelectComplete(onMouseDownPoint, point);
          }
        } else {
          props.onClick(point);
        }
        setOnMouseDownPoint(null);
      }}
      onMouseMove={(e: React.MouseEvent<HTMLCanvasElement, MouseEvent>) => {
        if (onMouseDownPoint != null) {
          const point = getClickPoint(e);
          props.onSelect(onMouseDownPoint, point);
        }
      }}
    />
  );
}

function getClickPoint(
  e: React.MouseEvent<HTMLCanvasElement, MouseEvent>,
): TsVec2 {
  const canvas = document.getElementById("canvas") as HTMLCanvasElement;
  const rect = canvas.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;

  // transform x and y to wgpu coordinate system which is from -1 to 1
  const wgpuX = (x / rect.width) * 2 - 1;
  const wgpuY = (y / rect.height) * -2 + 1;

  return { x: wgpuX, y: wgpuY };
}

function SimulationSlider({
  label,
  value,
  min,
  max,
  precision = 0,
  scale = "linear",
  onChange,
}: {
  label?: string;
  value: number;
  min: number;
  max: number;
  precision?: number;
  scale?: "linear" | "logarithmic";
  onChange: (value: number) => void;
}) {
  const [toLocalValue, fromLocalValue] = (() => {
    switch (scale) {
      case "logarithmic": {
        const LOG_COEFFICIENT = 4; // Make the scale a bit less extreme
        return [
          (value: number) => {
            if (value === 0) {
              return 0;
            }
            const sign = value < 0 ? -1 : 1;
            return sign * Math.log10(Math.abs(value) + 1) * LOG_COEFFICIENT;
          },
          (value: number) => {
            const sign = value < 0 ? -1 : 1;
            // biome-ignore lint/style/useExponentiationOperator: <explanation>
            return sign * Math.pow(10, value / LOG_COEFFICIENT) - 1;
          },
        ];
      }
      case "linear":
        return [(value: number) => value, (value: number) => value];
      default: {
        // exhaustive check
        const check: never = scale;
        throw new Error(`Unknown scale: ${check}`);
      }
    }
  })();

  const localValue = toLocalValue(value);
  const localMax = toLocalValue(max);
  const localMin = toLocalValue(min);
  const step = (localMax - localMin) / 100; // More steps for smoother feel

  const id =
    label != null
      ? `id-${label.toLowerCase().replace(/\s+/g, "-")}`
      : undefined;
  return (
    <div>
      {label != null && (
        <Label
          htmlFor={id}
          className="!text-sm !font-medium !mb-2 !text-foreground"
        >
          {`${label} (${formatNumber(value, 0, precision)})`}
        </Label>
      )}
      <Slider
        id={id}
        value={[localValue]}
        min={localMin}
        max={localMax}
        step={step}
        className="cursor-pointer"
        onValueChange={(values) => {
          const value = values[0];
          if (value == null) {
            return;
          }

          onChange(fromLocalValue(value));
        }}
      />
    </div>
  );
}
