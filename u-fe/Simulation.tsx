// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Play, Settings2, Split } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  get_selected_node_idxs,
  set_event_loop_active,
  set_simulation_params,
} from "../.build/wasm/unigraph_wasm.js";
import type { SelectionType } from "../u-be/unigraph_wgpu/bindings/SelectionType.js";
import type { SimulationParams } from "../u-be/unigraph_wgpu/bindings/SimulationParams";
import type { TsVec2 } from "../u-be/unigraph_wgpu/bindings/TsVec2.js";
import { Button } from "./components/ui/button.js";
import { Label } from "./components/ui/label";
import { Slider } from "./components/ui/slider";
import { Toggle } from "./components/ui/toggle";
import { useSelectedNodes } from "./context/SelectedNodesContext.js";
import { useSimulationParams } from "./context/SimulationParamsContext.js";

export default function Simulation() {
  const [paramsVisible, setParamsVisible] = useState(false);
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
    <div className="flex h-full border-r">
      {paramsVisible && (
        <ParamsPanel
          simulationParams={simulationParams}
          setSimulationParams={setSimulationParams}
        />
      )}

      <div className="flex w-[600px] grow-1 shrink-0 relative">
        <Canvas
          onClick={onCanvasClick}
          onSelect={onCanvasSelect}
          onSelectComplete={onCanvasSelectComplete}
        />
        <SimulationParamsToggle
          selected={paramsVisible}
          onSelectedChange={setParamsVisible}
        />
      </div>
    </div>
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

function ParamsPanel(props: {
  simulationParams: SimulationParams;
  setSimulationParams: (params: SimulationParams) => void;
}) {
  return (
    <div className="px-6 py-4 flex flex-col gap-4 w-48 bg-card">
      <div className="flex gap-4">
        <Toggle
          pressed={props.simulationParams.active}
          onPressedChange={(active) => {
            props.setSimulationParams({
              ...props.simulationParams,
              active,
            });
          }}
        >
          <Play />
        </Toggle>
        <Toggle
          pressed={props.simulationParams.render_edges}
          onPressedChange={(render_edges) => {
            props.setSimulationParams({
              ...props.simulationParams,
              render_edges,
            });
          }}
        >
          <Split />
        </Toggle>
      </div>
      <div className="flex flex-col gap-4 grow-1">
        <div>
          <Label
            htmlFor="antigravity-force-slider"
            className="text-sm font-medium mb-2"
          >
            Antigravity force
          </Label>
          <Slider
            id="antigravity-force-slider"
            defaultValue={[props.simulationParams.gravity_force_multiplier]}
            min={0.1}
            max={10.0}
            onValueChange={(values) => {
              const value = values[0];
              if (value == null) {
                return;
              }
              props.setSimulationParams({
                ...props.simulationParams,
                gravity_force_multiplier: value,
              });
            }}
          />
        </div>
        <div>
          <Label
            htmlFor="edge-force-slider"
            className="text-sm font-medium mb-2"
          >
            Edge force
          </Label>
          <Slider
            id="edge-force-slider"
            defaultValue={[props.simulationParams.edge_force_multiplier]}
            min={0.1}
            max={10.0}
            onValueChange={(values) => {
              const value = values[0];
              if (value == null) {
                return;
              }
              props.setSimulationParams({
                ...props.simulationParams,
                edge_force_multiplier: value,
              });
            }}
          />
        </div>
        <div>
          <Label
            htmlFor="max-velocity-slider"
            className="text-sm font-medium mb-2"
          >
            Max velocity
          </Label>
          <Slider
            id="max-velocity-slider"
            defaultValue={[props.simulationParams.max_velocity_multiplier]}
            min={0.1}
            max={10.0}
            onValueChange={(values) => {
              const value = values[0];
              if (value == null) {
                return;
              }
              props.setSimulationParams({
                ...props.simulationParams,
                max_velocity_multiplier: value,
              });
            }}
          />
        </div>
        <div>
          <Label
            htmlFor="node-size-slider"
            className="text-sm font-medium mb-2"
          >
            Node Size
          </Label>
          <Slider
            id="node-size-slider"
            defaultValue={[props.simulationParams.node_size_scale]}
            min={1}
            max={100}
            onValueChange={(values) => {
              const node_size_scale = values[0];
              if (node_size_scale == null) {
                return;
              }
              props.setSimulationParams({
                ...props.simulationParams,
                node_size_scale,
              });
            }}
          />
        </div>
        <div>
          <Label
            htmlFor="compute-forces-every-x-frames-slider"
            className="text-sm font-medium mb-2"
          >
            {props.simulationParams.compute_forces_every_n_frames} frame
            {props.simulationParams.compute_forces_every_n_frames === 1
              ? ""
              : "s"}
            /force compute
          </Label>
          <Slider
            id="compute-forces-every-x-frames-slider"
            defaultValue={[
              props.simulationParams.compute_forces_every_n_frames,
            ]}
            min={1}
            max={20}
            step={1}
            onValueChange={(values) => {
              const compute_forces_every_n_frames = values[0];
              if (compute_forces_every_n_frames == null) {
                return;
              }
              props.setSimulationParams({
                ...props.simulationParams,
                compute_forces_every_n_frames,
              });
            }}
          />
        </div>
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
