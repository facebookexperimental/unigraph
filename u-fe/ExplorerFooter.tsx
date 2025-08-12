// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  ArrowLeftRight,
  ArrowUpNarrowWide,
  ChartNoAxesCombined,
  CircleDollarSign,
  Layers,
  List,
  Network,
  Tally5,
  TreePalm,
  X,
} from "lucide-react";
import { useMemo } from "react";
import Metric from "./components/Metric";
import UHoverCard from "./components/UHoverCard";
import UToggleButton from "./components/UToggleButton";

import {
  KEYBOARD_SHORTCUTS,
  KeyboardShortcutLabel,
} from "./ExplorerKeyboardShortcutsWrapper";
import {
  useToggleDominatorTreeView,
  useToggleFlatListView,
  useToggleReverseView,
} from "./GraphStructureHooks";
import { H3 } from "./Typography";
import UTooltip from "./components/UTooltip";
import { Button } from "./components/ui/button";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraph } from "./context/NativeGraphContext";
import { useSelectedNodes } from "./context/SelectedNodesContext";
import { useTVC } from "./context/TraversalConfigContext";
import formatMetric from "./lib/formatMetric";
import formatNumber from "./lib/formatNumber";

export default function ExplorerFooter() {
  return (
    <div className="flex h-16 bg-card border-tw-full justify-between">
      <Toggles />
      <SelectedNodesMetrics />
    </div>
  );
}

function SelectedNodesMetrics() {
  const [graphSettings] = useGraphSettings();
  const [selectedNodes, _setSelectedNodes, resetSelectedNodes] =
    useSelectedNodes();
  const nativeGraph = useNativeGraph();

  const combinedMetrics = useMemo(() => {
    return nativeGraph.getCombinedMetrics(selectedNodes);
  }, [nativeGraph, selectedNodes]);

  if (combinedMetrics == null) {
    return null;
  }

  if (selectedNodes.length === 0) {
    return null;
  }

  const metrics = Object.entries(combinedMetrics.metrics).map(
    ([metricName, value]) => {
      const format =
        graphSettings?.ui_settings?.columns?.metric_settings?.[metricName]
          ?.format;
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
      const format =
        graphSettings?.ui_settings?.columns?.metric_settings?.[metricName]
          ?.format;

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
      <UTooltip tooltip="Clear node selection">
        <Button
          className="h-8 cursor-pointer"
          variant="ghost"
          size="icon"
          onClick={resetSelectedNodes}
        >
          <X />
        </Button>
      </UTooltip>
      <Metric
        label="Selected"
        value={formatNumber(selectedNodes.length)}
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

  const [flatViewEnabled, toggleFlatListView] = useToggleFlatListView();
  const [reverseViewEnabled, toggleReverseView] = useToggleReverseView();
  const [dominatorTreeViewEnabled, toggleDominatorTreeView] =
    useToggleDominatorTreeView();

  return (
    <div className="flex gap-4 items-center m-4">
      <UHoverCard content={<TransitiveHovercardContent />}>
        <UToggleButton
          size="sm"
          selected={
            graphSettings.ui_settings?.columns?.show_transitive === true
          }
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
      </UHoverCard>

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

      <UHoverCard content={<MetricsHovercardContent />}>
        <UToggleButton
          size="sm"
          selected={graphSettings.ui_settings?.columns?.hide_metrics !== true}
          onSelectedChange={(checked) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  hide_metrics: !checked,
                },
              },
            });
          }}
        >
          <ChartNoAxesCombined />
        </UToggleButton>
      </UHoverCard>

      {hasTiers && (
        <UHoverCard content={<TiersHoverCardContent />}>
          <UToggleButton
            size="sm"
            selected={graphSettings.ui_settings?.columns?.show_tiered === true}
            onSelectedChange={(checked) => {
              setGraphSettings({
                ...graphSettings,
                ui_settings: {
                  ...graphSettings.ui_settings,
                  columns: {
                    ...graphSettings.ui_settings?.columns,
                    show_tiered: checked,
                  },
                },
              });
            }}
          >
            <Layers />
          </UToggleButton>
        </UHoverCard>
      )}

      <div className="border-l border border-accent h-full w-0" />

      <UToggleButton
        tooltip={
          <span>
            Show as a flat list{" "}
            <KeyboardShortcutLabel
              label={KEYBOARD_SHORTCUTS.FLAT_LIST.toUpperCase()}
            />
          </span>
        }
        size="sm"
        selected={flatViewEnabled}
        onSelectedChange={toggleFlatListView}
      >
        <List />
      </UToggleButton>

      <UToggleButton
        tooltip={
          <span>
            Show reverse graph (children to parents)
            <KeyboardShortcutLabel
              label={KEYBOARD_SHORTCUTS.REVERSE_GRAPH.toUpperCase()}
            />
          </span>
        }
        size="sm"
        selected={reverseViewEnabled}
        onSelectedChange={toggleReverseView}
      >
        <ArrowLeftRight />
      </UToggleButton>
      <UToggleButton
        tooltip={
          <span>
            Show as a dominator tree
            <KeyboardShortcutLabel
              label={KEYBOARD_SHORTCUTS.DOMINATOR_TREE.toUpperCase()}
            />
          </span>
        }
        size="sm"
        selected={dominatorTreeViewEnabled}
        onSelectedChange={toggleDominatorTreeView}
      >
        <TreePalm />
      </UToggleButton>
    </div>
  );
}

function ConjointCostHoverCardContent() {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const nativeGraph = useNativeGraph();

  const metricCards = Object.entries(
    graphSettings?.ui_settings?.columns?.metric_settings ?? {},
  ).map(([metricName, metricSettings]) => {
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
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  show_conjoint: true,
                  metric_settings: {
                    ...graphSettings?.ui_settings?.columns?.metric_settings,
                    [metricName]: {
                      ...metricSettings,
                      show_conjoint_tiered: {
                        ...metricSettings?.show_conjoint_tiered,
                        [tierName]: selected ? "WhenEnabledGlobally" : "Never",
                      },
                    },
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
      <div
        key={`conjoint-metric-${metricName}`}
        className="flex gap-2 flex-wrap"
      >
        <UToggleButton
          key={`conjoint-self-${metricName}`}
          size="sm"
          tooltip={`Conjoint cost of transitive values of '${metricName}' metric`}
          selected={metricSettings?.show_conjoint_self !== "Never"}
          onSelectedChange={(selected) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  show_conjoint: true,
                  metric_settings: {
                    ...graphSettings?.ui_settings?.columns?.metric_settings,
                    [metricName]: {
                      ...metricSettings,
                      show_conjoint_self: selected
                        ? "WhenEnabledGlobally"
                        : "Never",
                    },
                  },
                },
              },
            });
          }}
        >
          <span className="text-sm">{metricName}</span>
        </UToggleButton>
        {tiers}
      </div>
    );
  });

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
        {`conj(A) = (
    1_for_self +
    A.children.map(
      child -> conj(child)
    ).sum()
) / A.parents.length`}
      </pre>
      <p>
        This way people will be penalized less for things that are popular. E.g.
        if there is a popular framework that almost every single node uses it
        would not make sense for it to try to remove that depenedncy, since it
        will likely still stay in the graph.
      </p>
      <p>
        Best way to use this metric is to show the graph as a flat list, order
        by conjoint cost "descending" and then look for nodes that have high
        conjoint cost but don't seem like they should be there.
      </p>
      <div className="flex gap-2 flex-wrap">
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
                  show_conjoint: true,
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

function TransitiveHovercardContent() {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const nativeGraph = useNativeGraph();

  const metricCards = nativeGraph.metricNames.map((metricName) => {
    const metricSettings =
      graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};
    return (
      <UToggleButton
        key={`${metricName}`}
        size="sm"
        tooltip={`Show a column with the values of '${metricName}' metric`}
        selected={
          metricSettings?.column_show_transitive === "WhenEnabledGlobally"
        }
        onSelectedChange={(selected) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              columns: {
                ...graphSettings.ui_settings?.columns,
                // if we select something under the transitive card
                // we probably want to show these automatically to avoid
                // "why is it not doing anything??" confusion
                show_transitive: true,
                metric_settings: {
                  ...graphSettings?.ui_settings?.columns?.metric_settings,
                  [metricName]: {
                    ...metricSettings,
                    column_show_transitive: selected
                      ? "WhenEnabledGlobally"
                      : "Never",
                  },
                },
              },
            },
          });
        }}
      >
        <span className="text-sm">{`${metricName}`}</span>
      </UToggleButton>
    );
  });

  return (
    <div className="flex flex-col gap-2">
      <H3 text="Metric Columns" />
      <div className="flex flex-wrap gap-2">
        <UToggleButton
          size="sm"
          tooltip="Show transitive children count"
          selected={
            graphSettings.ui_settings?.columns?.show_transitive_count !==
            "Never"
          }
          onSelectedChange={(selected) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  show_transitive: true,
                  show_transitive_count: selected
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

function MetricsHovercardContent() {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const nativeGraph = useNativeGraph();

  const metricCards = nativeGraph.metricNames.map((metricName) => {
    const metricSettings =
      graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};
    return (
      <UToggleButton
        key={`${metricName}`}
        size="sm"
        tooltip={`Show a column with the values of '${metricName}' metric`}
        selected={metricSettings?.column_hide_self !== true}
        onSelectedChange={(selected) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              columns: {
                ...graphSettings.ui_settings?.columns,
                // if we select something under the metrics card
                // we probably want to show these automatically to avoid
                // "why is it not doing anything??" confusion
                hide_metrics: false,
                metric_settings: {
                  ...graphSettings?.ui_settings?.columns?.metric_settings,
                  [metricName]: {
                    ...metricSettings,
                    column_hide_self: !selected,
                  },
                },
              },
            },
          });
        }}
      >
        <span className="text-sm">{`${metricName}`}</span>
      </UToggleButton>
    );
  });

  return (
    <div className="flex flex-col gap-2">
      <H3 text="Metric Columns" />
      <div className="flex flex-wrap gap-2">{metricCards}</div>
    </div>
  );
}

function TiersHoverCardContent() {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const nativeGraph = useNativeGraph();
  const allTiers = nativeGraph.stats().tier_names;
  const { tvc, setTvc } = useTVC();

  const tieredmetricCards = nativeGraph.metricNames.map((metricName) => {
    const metricSettings =
      graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};
    const tiers = allTiers.map((tierName) => {
      return (
        <UToggleButton
          key={`tiered-${metricName}-${tierName}`}
          size="sm"
          tooltip={`Show a column for transitive values of '${metricName}' metric for ${tierName} tier`}
          selected={metricSettings?.column_show_tiered?.[tierName] !== "Never"}
          onSelectedChange={(selected) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  metric_settings: {
                    ...graphSettings?.ui_settings?.columns?.metric_settings,
                    [metricName]: {
                      ...metricSettings,
                      column_show_tiered: {
                        ...metricSettings?.column_show_tiered,
                        [tierName]: selected ? "WhenEnabledGlobally" : "Never",
                      },
                    },
                  },
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
      <div key={metricName} className="flex flex-col gap-2">
        <H3 className="text-muted-foreground" text={`${metricName} Tiers`} />
        <div className="flex flex-wrap gap-2">{tiers}</div>
      </div>
    );
  });

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
          let newGraphSettings = { ...graphSettings };

          for (const metricName of Object.keys(
            newGraphSettings?.ui_settings?.columns?.metric_settings ?? {},
          )) {
            /// When max tier is selected we want to hide columns for all tiers above it, because
            /// their value will be 0 anyway and showing it will clutter the UI and make it confusing.
            for (let idx = 0; idx < allTiers.length; idx++) {
              const tierName = allTiers[idx] as string;
              const value = idx > tierIDX ? "Never" : "WhenEnabledGlobally";
              newGraphSettings = {
                ...newGraphSettings,
                ui_settings: {
                  ...newGraphSettings.ui_settings,
                  columns: {
                    ...newGraphSettings?.ui_settings?.columns,
                    metric_settings: {
                      ...newGraphSettings?.ui_settings?.columns
                        ?.metric_settings,
                      [metricName]: {
                        ...newGraphSettings?.ui_settings?.columns
                          ?.metric_settings?.[metricName],
                        column_show_tiered: {
                          ...newGraphSettings?.ui_settings?.columns
                            ?.metric_settings?.[metricName]?.column_show_tiered,
                          [tierName]: value,
                        },
                      },
                    },
                  },
                },
              };
            }
          }

          setGraphSettings(newGraphSettings);
          setTvc({
            ...tvc,
            tiered_traversal: {
              AscendingTiers: {
                tiers: tvc.tiered_traversal?.AscendingTiers?.tiers ?? [],
                max_tier: selected ? tierIDX : undefined,
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
      <H3 text="Tiered Metric Columns" />
      <div className="flex flex-col gap-2">{tieredmetricCards}</div>
      <H3 text="Max Tier" />
      <div className="flex flex-wrap gap-2">{tierSwitches}</div>
    </div>
  );
}
