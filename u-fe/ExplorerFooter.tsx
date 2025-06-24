import {
  ArrowUpNarrowWide,
  CircleDollarSign,
  List,
  Network,
  TreePine,
} from "lucide-react";
import { useMemo } from "react";
import type { CombinedMetricsForNodes } from "u-be/unigraph_core/bindings/CombinedMetricsForNodes";
import Metric from "./components/Metric";
import UToggleButton from "./components/UToggleButton";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraph } from "./context/NativeGraphContext";
import formatMetric from "./lib/formatMetric";
import formatNumber from "./lib/formatNumber";
import type { NodeIDX } from "./types";

export default function ExplorerFooter({
  selectedNodeIDXs,
}: { selectedNodeIDXs: NodeIDX[] }) {
  const nativeGraph = useNativeGraph();

  const combinedMetrics = useMemo(() => {
    return nativeGraph.getCombinedMetrics(selectedNodeIDXs);
  }, [nativeGraph, selectedNodeIDXs]);

  return (
    <div className="flex h-16 bg-card border-tw-full justify-between">
      <Toggles />
      <SelectedNodesMetrics
        combinedMetrics={combinedMetrics}
        selectedNodeIDXs={selectedNodeIDXs}
      />
    </div>
  );
}

function SelectedNodesMetrics({
  selectedNodeIDXs,
  combinedMetrics,
}: {
  selectedNodeIDXs: NodeIDX[];
  combinedMetrics: CombinedMetricsForNodes | null;
}) {
  const [graphSettings] = useGraphSettings();
  if (combinedMetrics == null) {
    return null;
  }

  if (selectedNodeIDXs.length === 0) {
    return null;
  }

  const metrics = Object.entries(combinedMetrics.metrics).map(
    ([metricName, value]) => {
      const format = graphSettings.metric_settings?.[metricName]?.format;
      const formatted = formatMetric(value ?? 0, format);
      return (
        <Metric
          label={metricName}
          value={formatted}
          metricSize="text-sm"
          key={`metric: ${metricName}`}
        />
      );
    },
  );

  const tieredMetrics = Object.entries(combinedMetrics.tiered_metrics).flatMap(
    ([metricName, tiered]) => {
      const format = graphSettings.metric_settings?.[metricName]?.format;

      if (tiered == null) {
        return [];
      }

      return Object.entries(tiered).map(([tier, value]) => {
        const formatted = formatMetric(value ?? 0, format);
        return (
          <Metric
            label={tier}
            value={formatted}
            metricSize="text-sm"
            key={`tiered metric: ${metricName} ${tier}`}
          />
        );
      });
    },
  );

  return (
    <div className="flex flex-wrap gap-4 mx-4 h-full items-center">
      <Metric
        label="Selected"
        value={formatNumber(selectedNodeIDXs.length)}
        metricSize="text-sm"
      />
      {metrics}
      {tieredMetrics}
    </div>
  );
}

function Toggles() {
  const [graphSettings, setGraphSettings] = useGraphSettings();

  return (
    <div className="flex gap-4 items-center m-4">
      <UToggleButton
        tooltip="Show number of transitive children nodes"
        size="sm"
        selected={
          graphSettings.ui_settings?.columns?.show_transitive_count === true
        }
        onSelectedChange={(checked) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              columns: {
                ...graphSettings.ui_settings?.columns,
                show_transitive_count: checked,
              },
            },
          });
        }}
      >
        <Network />
      </UToggleButton>
      <UToggleButton
        tooltip="Show conjoint cost"
        size="sm"
        selected={
          graphSettings.ui_settings?.columns?.show_conjoint_count === true
        }
        onSelectedChange={(checked) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              columns: {
                ...graphSettings.ui_settings?.columns,
                show_conjoint_count: checked,
              },
            },
          });
        }}
      >
        <CircleDollarSign />
      </UToggleButton>
      <UToggleButton
        tooltip="Show number of parent nodes"
        size="sm"
        selected={
          graphSettings.ui_settings?.columns?.show_parents_count === true
        }
        onSelectedChange={(checked) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              columns: {
                ...graphSettings.ui_settings?.columns,
                show_parents_count: checked,
              },
            },
          });
        }}
      >
        <ArrowUpNarrowWide />
      </UToggleButton>
      <UToggleButton
        tooltip="Show as a flat list"
        size="sm"
        selected={graphSettings.ui_settings?.show_as_a_flat_list === true}
        onSelectedChange={(checked) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              show_as_a_flat_list: checked,
            },
          });
        }}
      >
        <List />
      </UToggleButton>
      <UToggleButton
        tooltip="Show as a dominator tree"
        size="sm"
        selected={graphSettings.ui_settings?.show_as_dominator_tree === true}
        onSelectedChange={(checked) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              show_as_dominator_tree: checked,
            },
          });
        }}
      >
        <TreePine />
      </UToggleButton>
    </div>
  );
}
