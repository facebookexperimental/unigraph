import {
  ArrowUpNarrowWide,
  CircleDollarSign,
  Layers,
  List,
  Network,
  Tally5,
  TreePalm,
} from "lucide-react";
import { useMemo } from "react";
import type { CombinedMetricsForNodes } from "u-be/unigraph_core/bindings/CombinedMetricsForNodes";
import Metric from "./components/Metric";
import UHoverCard from "./components/UHoverCard";
import UToggleButton from "./components/UToggleButton";

import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraph } from "./context/NativeGraphContext";
import { useTVC } from "./context/TraversalConfigContext";
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
  const nativeGraph = useNativeGraph();
  const hasTiers = nativeGraph.stats().tier_names.length > 0;

  return (
    <div className="flex gap-4 items-center m-4">
      <UToggleButton
        tooltip="Show number of transitive children nodes"
        size="sm"
        selected={graphSettings.ui_settings?.columns?.show_transitive === true}
        onSelectedChange={(checked) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              columns: {
                ...graphSettings.ui_settings?.columns,
                show_transitive: checked,
              },
            },
          });
        }}
      >
        <Network />
      </UToggleButton>
      <UHoverCard content={<ConjointCostHoverCardContent />}>
        <UToggleButton
          size="sm"
          selected={graphSettings.ui_settings?.columns?.show_conjoint === true}
          onSelectedChange={(checked) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  show_conjoint: checked,
                },
              },
            });
          }}
        >
          <CircleDollarSign />
        </UToggleButton>
      </UHoverCard>
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
        <TreePalm />
      </UToggleButton>
      {hasTiers && (
        <UHoverCard content={<TiersHoverCardContent />}>
          <UToggleButton
            size="sm"
            selected={graphSettings.ui_settings?.columns?.hide_tiered !== true}
            onSelectedChange={(checked) => {
              setGraphSettings({
                ...graphSettings,
                ui_settings: {
                  ...graphSettings.ui_settings,
                  columns: {
                    ...graphSettings.ui_settings?.columns,
                    hide_tiered: !checked,
                  },
                },
              });
            }}
          >
            <Layers />
          </UToggleButton>
        </UHoverCard>
      )}
    </div>
  );
}

function ConjointCostHoverCardContent() {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const nativeGraph = useNativeGraph();

  const metricCards = Object.entries(graphSettings.metric_settings ?? {}).map(
    ([metricName, metricSettings]) => {
      const tiers = nativeGraph.stats().tier_names.map((tierName) => {
        return (
          <UToggleButton
            key={`conjoint-tiered-${metricName}-${tierName}`}
            size="sm"
            tooltip={`Conjoint cost of transitive values of '${metricName}' metric for ${tierName} tier`}
            selected={
              metricSettings?.show_conjoint_tiered?.[tierName] !== "Never"
            }
            onSelectedChange={(selected) => {
              setGraphSettings({
                ...graphSettings,
                metric_settings: {
                  ...graphSettings.metric_settings,
                  [metricName]: {
                    ...metricSettings,
                    show_conjoint_tiered: {
                      ...metricSettings?.show_conjoint_tiered,
                      [tierName]: selected ? "WhenEnabledGlobally" : "Never",
                    },
                  },
                },
              });
            }}
          >
            <span className="text-sm">{`${metricName}: ${tierName}`}</span>
          </UToggleButton>
        );
      });

      return (
        <>
          <UToggleButton
            key={`conjoint-self-${metricName}`}
            size="sm"
            tooltip={`Conjoint cost of transitive values of '${metricName}' metric`}
            selected={metricSettings?.show_conjoint_self !== "Never"}
            onSelectedChange={(selected) => {
              setGraphSettings({
                ...graphSettings,
                metric_settings: {
                  ...graphSettings.metric_settings,
                  [metricName]: {
                    ...metricSettings,
                    show_conjoint_self: selected
                      ? "WhenEnabledGlobally"
                      : "Never",
                  },
                },
              });
            }}
          >
            <span className="text-sm">{metricName}</span>
          </UToggleButton>
          {tiers}
        </>
      );
    },
  );

  return (
    <div className="flex flex-col gap-2">
      <p>
        Conjoint cost of a node is a value that represents its transitive size
        adjusted for how many other nodes it depends on.
      </p>
      <p>
        It's calculated by summing up the cost of all ConjCost(direct children)
        and dividing it by the number of parents.
      </p>
      <pre className="text-wrap break-words bg-secondary rounded-md p-2">
        {
          "conj(A) = (1_for_self + A.children.map(child -> conj(child)).sum()) / A.parents.length"
        }
      </pre>
      <p>
        This way people will be penalized less for things that are popular. E.g.
        if there is a popular framework that almost every single node uses it
        would not make sense for it to try to remove that depenedncy, since it
        will likely still stay in the graph.
      </p>
      <div className="flex gap-2">
        <UToggleButton
          size="sm"
          tooltip="Conjoint cost of simple transitive children count"
          selected={
            graphSettings.ui_settings?.columns?.show_conjoint_count !== "Never"
          }
          onSelectedChange={(selected) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  show_conjoint_count: selected
                    ? "WhenEnabledGlobally"
                    : "Never",
                },
              },
            });
          }}
        >
          <Tally5 />
        </UToggleButton>
        {metricCards}
      </div>
    </div>
  );
}

function TiersHoverCardContent() {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const nativeGraph = useNativeGraph();
  const allTiers = nativeGraph.stats().tier_names;
  const { tvc, setTvc } = useTVC();

  const metricCards = Object.entries(graphSettings.metric_settings ?? {}).map(
    ([metricName, metricSettings]) => {
      const tiers = allTiers.map((tierName) => {
        return (
          <UToggleButton
            key={`tiered-${metricName}-${tierName}`}
            size="sm"
            tooltip={`Transitive values of '${metricName}' metric for ${tierName} tier`}
            selected={
              metricSettings?.column_show_tiered?.[tierName] !== "Never"
            }
            onSelectedChange={(selected) => {
              setGraphSettings({
                ...graphSettings,
                metric_settings: {
                  ...graphSettings.metric_settings,
                  [metricName]: {
                    ...metricSettings,
                    column_show_tiered: {
                      ...metricSettings?.column_show_tiered,
                      [tierName]: selected ? "WhenEnabledGlobally" : "Never",
                    },
                  },
                },
              });
            }}
          >
            <span className="text-sm">{`${metricName}: ${tierName}`}</span>
          </UToggleButton>
        );
      });

      return tiers;
    },
  );

  const tierSwitches = allTiers.map((tierName, tierIDX) => {
    const selected = tvc.tiered_traversal?.AscendingTiers?.max_tier === tierIDX;

    return (
      <UToggleButton
        // biome-ignore lint/suspicious/noArrayIndexKey: <explanation>
        key={`tier-${tierIDX}`}
        size="sm"
        tooltip={`Only show nodes that are on or below '${tierName}'`}
        selected={selected}
        onSelectedChange={(selected) => {
          setTvc({
            ...tvc,
            tiered_traversal: {
              AscendingTiers: {
                tiers: tvc.tiered_traversal?.AscendingTiers?.tiers ?? [],
                max_tier: selected ? tierIDX : null,
              },
            },
          });
        }}
      >
        <span className="text-sm">{tierName}</span>
      </UToggleButton>
    );
  });

  return (
    <div className="flex flex-col gap-2">
      <div className="flex gap-2">{metricCards}</div>
      <h2 className="text-xl">Max Tier</h2>
      <div className="flex flex-wrap gap-2">{tierSwitches}</div>
    </div>
  );
}
