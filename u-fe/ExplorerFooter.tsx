// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  ArrowLeftRight,
  ArrowUpNarrowWide,
  ArrowUpToLine,
  ChartNoAxesCombined,
  CircleDollarSign,
  FileDiff,
  Layers,
  List,
  Network,
  Tally5,
  ToggleLeft,
  ToggleRight,
  TreePalm,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import Metric from "./components/Metric";
import UHoverCard from "./components/UHoverCard";
import UToggleButton from "./components/UToggleButton";
import UTooltip from "./components/UTooltip";
import { Button } from "./components/ui/button";
import { useTreeTableRef } from "./context/GlobalElementRefs";
import {
  KEYBOARD_SHORTCUTS,
  KeyboardShortcutLabel,
} from "./context/GlobalKeyboardShortcutsContext";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraphL, useTwinGraph } from "./context/NativeGraphContext";
import { useSelectedNodes } from "./context/SelectedNodesContext";
import { useTVC } from "./context/TraversalConfigContext";
import {
  useToggleDominatorTreeView,
  useToggleFlatListView,
  useToggleReverseView,
} from "./GraphStructureHooks";
import ConjointCostDocs from "./inline_docs/ConjointCost";
import formatMetric from "./lib/formatMetric";
import formatNumber from "./lib/formatNumber";
import NodeSearch from "./NodeSearch";
import { H2, H3 } from "./Typography";
import { isEnabledForGraphStructure } from "./tree_table/columns/ColumnUtils";

export default function ExplorerFooter() {
  return (
    <div className="flex h-16 shrink-0 bg-card border-t justify-between items-center px-4 gap-8">
      <Toggles />
      <NodeSearch />
      <SelectedNodesMetrics />
      <BackToTopButton />
    </div>
  );
}

function SelectedNodesMetrics() {
  const [graphSettings] = useGraphSettings();
  const [selectedNodes, _setSelectedNodes, resetSelectedNodes] =
    useSelectedNodes();
  const nativeGraph = useNativeGraphL();

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
  const nativeGraph = useNativeGraphL();
  const hasMetrics = nativeGraph.metricNames.length > 0;

  const [flatViewEnabled, toggleFlatListView] = useToggleFlatListView();
  const [reverseViewEnabled, toggleReverseView] = useToggleReverseView();
  const [dominatorTreeViewEnabled, toggleDominatorTreeView] =
    useToggleDominatorTreeView();

  return (
    <div className="flex gap-4 items-center">
      <UHoverCard content={<CountsHovercardContent />}>
        <UToggleButton
          size="sm"
          selected={graphSettings.ui_settings?.columns?.show_counts === true}
          onSelectedChange={(checked) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  show_counts: checked,
                },
              },
            });
          }}
        >
          <Tally5 />
        </UToggleButton>
      </UHoverCard>

      {hasMetrics && (
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
      )}

      <SeparatorVertical />

      <UToggleButton
        tooltip={
          <span>
            Show as a flat list{" "}
            <KeyboardShortcutLabel shortcut={KEYBOARD_SHORTCUTS.FLAT_LIST} />
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
              shortcut={KEYBOARD_SHORTCUTS.REVERSE_GRAPH}
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
              shortcut={KEYBOARD_SHORTCUTS.DOMINATOR_TREE}
            />
          </span>
        }
        size="sm"
        selected={dominatorTreeViewEnabled}
        onSelectedChange={toggleDominatorTreeView}
      >
        <TreePalm />
      </UToggleButton>
      <ChangedNodesOnlyToggle />
    </div>
  );
}

function CountsHovercardContent() {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const twinGraph = useTwinGraph();
  const singleGraph = twinGraph.r == null;

  return (
    <div className="flex flex-col gap-2">
      <H3 text="Node count columns" />
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
                  show_counts: true,
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

        {singleGraph && (
          <>
            <UToggleButton
              tooltip="Show number of parent nodes"
              size="sm"
              selected={
                graphSettings.ui_settings?.columns?.show_parents_count ===
                "WhenEnabledGlobally"
              }
              onSelectedChange={(checked) => {
                setGraphSettings({
                  ...graphSettings,
                  ui_settings: {
                    ...graphSettings.ui_settings,
                    columns: {
                      ...graphSettings.ui_settings?.columns,
                      show_counts: true,
                      show_parents_count: checked
                        ? "WhenEnabledGlobally"
                        : "Never",
                    },
                  },
                });
              }}
            >
              <ArrowUpNarrowWide />
            </UToggleButton>

            <UToggleButton
              tooltip="Show dominated nodes counts"
              size="sm"
              selected={
                graphSettings.ui_settings?.columns?.show_dominated_count ===
                "WhenEnabledGlobally"
              }
              onSelectedChange={(checked) => {
                setGraphSettings({
                  ...graphSettings,
                  ui_settings: {
                    ...graphSettings.ui_settings,
                    columns: {
                      ...graphSettings.ui_settings?.columns,
                      show_counts: true,
                      show_dominated_count: checked
                        ? "WhenEnabledGlobally"
                        : "Never",
                    },
                  },
                });
              }}
            >
              <TreePalm />
            </UToggleButton>

            <UHoverCard content={<ConjointCostDocs />}>
              <UToggleButton
                size="sm"
                selected={
                  graphSettings.ui_settings?.columns?.show_conjoint_count ===
                  "WhenEnabledGlobally"
                }
                onSelectedChange={(selected) => {
                  setGraphSettings({
                    ...graphSettings,
                    ui_settings: {
                      ...graphSettings.ui_settings,
                      columns: {
                        ...graphSettings.ui_settings?.columns,
                        show_counts: true,
                        show_conjoint_count: selected
                          ? "WhenEnabledGlobally"
                          : "Never",
                      },
                    },
                  });
                }}
              >
                <CircleDollarSign />
              </UToggleButton>
            </UHoverCard>
          </>
        )}
      </div>
    </div>
  );
}

function MetricsHovercardContent() {
  const twinGraph = useTwinGraph();
  const [graphSettings, setGraphSettings] = useGraphSettings();

  const hasTiers = twinGraph.l.stats().tier_names.length > 0;
  const metricCards = twinGraph.l.metricNames.map((metricName) => {
    return <MetricCard key={metricName} metricName={metricName} />;
  });

  const tierColumnSelected =
    graphSettings?.ui_settings?.columns?.show_tier_column === true;

  return (
    <div className="flex flex-col gap-2">
      {hasTiers && (
        <>
          <H3 text="Tier Column" />
          <UToggleButton
            key={`show-tier-column`}
            size="sm"
            tooltip={`Show a column displaying node's tier`}
            selected={tierColumnSelected}
            onSelectedChange={(selected) => {
              setGraphSettings({
                ...graphSettings,
                ui_settings: {
                  ...graphSettings.ui_settings,
                  columns: {
                    ...graphSettings.ui_settings?.columns,
                    show_tier_column: selected ? true : undefined,
                  },
                },
              });
            }}
          >
            {"Enable"}
            {tierColumnSelected ? <ToggleRight /> : <ToggleLeft />}
          </UToggleButton>
        </>
      )}
      <H3 text="Metric Columns" />
      <div className="flex flex-wrap gap-2">{metricCards}</div>
      {hasTiers && <MaxTierSelector />}
    </div>
  );
}

function MaxTierSelector() {
  const nativeGraph = useNativeGraphL();
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const { tvcL, setTvcL, tvcR, setTvcR } = useTVC();
  const allTiers = nativeGraph.stats().tier_names;
  const maxTiers = allTiers.map((tierName, tierIDX) => {
    const selected =
      tvcL.tiered_traversal?.AscendingTiers?.max_tier === tierIDX;

    return (
      <UToggleButton
        // biome-ignore lint/suspicious/noArrayIndexKey: because
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
                        show_conjoint_tiered: {
                          ...newGraphSettings?.ui_settings?.columns
                            ?.metric_settings?.[metricName]
                            ?.show_conjoint_tiered,
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
          setTvcL({
            ...tvcL,
            tiered_traversal: {
              AscendingTiers: {
                tiers: tvcL.tiered_traversal?.AscendingTiers?.tiers ?? [],
                max_tier: selected ? tierIDX : undefined,
              },
            },
          });
          setTvcR({
            ...tvcR,
            tiered_traversal: {
              AscendingTiers: {
                tiers: tvcR?.tiered_traversal?.AscendingTiers?.tiers ?? [],
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
    <>
      <H3 text="Max Tier" />
      <div className="flex gap-2 flex-wrap">{maxTiers}</div>
    </>
  );
}

function MetricCard({ metricName }: { metricName: string }) {
  const nativeGraph = useNativeGraphL();
  const twinGraph = useTwinGraph();
  const singleGraph = twinGraph.r == null;

  const allTiers = nativeGraph.stats().tier_names;
  const hasTiers = allTiers.length > 0;

  const tiers = allTiers.map((tierName) => (
    <ToggleTierForMetric
      key={`${metricName}-${tierName}`}
      tierName={tierName}
      metricName={metricName}
    />
  ));

  const dominatedTiered = allTiers.map((tierName) => (
    <DominatedForTierForMetric
      key={`${metricName}-dominated-${tierName}`}
      tierName={tierName}
      metricName={metricName}
    />
  ));

  const conjointTiered = allTiers.map((tierName) => (
    <ToggleConjointForTierForMetric
      key={`conjoint-${metricName}-${tierName}`}
      tierName={tierName}
      metricName={metricName}
    />
  ));

  return (
    <div className="w-full flex flex-col gap-2">
      <SeparatorHorizontal />
      <H2 text={`${metricName}`} />
      <div className="flex flex-col gap-2 flex-wrap">
        <div className="flex  gap-2 flex-wrap">
          <EnableSelfMetricToggle metricName={metricName} />
          <EnableTransitiveMetricToggle metricName={metricName} />
          {singleGraph && (
            <EnableDominatedMetricToggle metricName={metricName} />
          )}
        </div>
        {hasTiers && (
          <>
            <H3 text="Tiers" />
            <div className="flex gap-2 flex-wrap">
              <EnableTieredMetricsToggle />
              {tiers}
            </div>
          </>
        )}

        {singleGraph && hasTiers && (
          <>
            <div className="flex gap-2 flex-wrap">
              <DominatedTieredToggle />
              {dominatedTiered}
            </div>
            <div className="flex gap-2 flex-wrap">
              <ConjointTieredToggle />
              {conjointTiered}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function ToggleTierForMetric({
  tierName,
  metricName,
}: {
  tierName: string;
  metricName: string;
}) {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const metricSettings =
    graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};
  return (
    <UToggleButton
      key={`tiered-${metricName}-${tierName}`}
      size="sm"
      tooltip={`Show a column for transitive values of '${metricName}' metric for ${tierName} tier`}
      selected={
        metricSettings?.column_show_tiered?.[tierName] === "WhenEnabledGlobally"
      }
      onSelectedChange={(selected) => {
        setGraphSettings({
          ...graphSettings,
          ui_settings: {
            ...graphSettings.ui_settings,
            columns: {
              ...graphSettings.ui_settings?.columns,
              hide_metrics: false,
              show_tiered_metrics: true,
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
}

function ToggleConjointForTierForMetric({
  tierName,
  metricName,
}: {
  tierName: string;
  metricName: string;
}) {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const metricSettings =
    graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};
  return (
    <UToggleButton
      key={`conjoint-tiered-${metricName}-${tierName}`}
      size="sm"
      tooltip={`Conjoint cost of transitive values of '${metricName}' metric for ${tierName} tier`}
      selected={
        metricSettings?.show_conjoint_tiered?.[tierName] ===
        "WhenEnabledGlobally"
      }
      onSelectedChange={(selected) => {
        setGraphSettings({
          ...graphSettings,
          ui_settings: {
            ...graphSettings.ui_settings,
            columns: {
              ...graphSettings.ui_settings?.columns,
              hide_metrics: false,
              show_conjoint_tiered_metrics: true,
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
      {tierName}
    </UToggleButton>
  );
}
function DominatedForTierForMetric({
  tierName,
  metricName,
}: {
  tierName: string;
  metricName: string;
}) {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const metricSettings =
    graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};

  const selected = isEnabledForGraphStructure(
    graphSettings?.ui_settings?.graph_structure,
    metricSettings?.show_dominated_tiered?.[tierName],
  );

  return (
    <UToggleButton
      key={`dominated-tiered-${metricName}-${tierName}`}
      size="sm"
      tooltip={`Dominated value for'${metricName}' metric for ${tierName} tier`}
      selected={selected}
      onSelectedChange={(selected) => {
        setGraphSettings({
          ...graphSettings,
          ui_settings: {
            ...graphSettings.ui_settings,
            columns: {
              ...graphSettings.ui_settings?.columns,
              hide_metrics: false,
              hide_dominated_tiered_metrics: false,
              metric_settings: {
                ...graphSettings?.ui_settings?.columns?.metric_settings,
                [metricName]: {
                  ...metricSettings,
                  show_dominated_tiered: {
                    ...metricSettings?.show_dominated_tiered,
                    [tierName]: selected ? "WhenEnabledGlobally" : "Never",
                  },
                },
              },
            },
          },
        });
      }}
    >
      {tierName}
    </UToggleButton>
  );
}

function BackToTopButton() {
  const treeTableRef = useTreeTableRef();

  const [isEnabled, setIsEnabled] = useState(false);

  useEffect(() => {
    treeTableRef.current?.addEventListener("scroll", () => {
      const scrollTop = treeTableRef.current?.scrollTop;
      setIsEnabled(scrollTop == null || scrollTop > 100);
    });
  }, [treeTableRef]);

  const handleClick = () => {
    treeTableRef.current?.scrollTo(0, 0);
  };

  return (
    <UTooltip tooltip="Scroll Back to Top">
      <Button
        className="cursor-pointer"
        disabled={!isEnabled}
        variant={"secondary"}
        onClick={handleClick}
      >
        <ArrowUpToLine />
      </Button>
    </UTooltip>
  );
}

function ChangedNodesOnlyToggle() {
  const twinGraph = useTwinGraph();
  const [graphSettings, setGraphSettings] = useGraphSettings();

  if (twinGraph.r == null) {
    return null;
  }

  return (
    <>
      <SeparatorVertical />
      <UToggleButton
        className="cursor-pointer"
        tooltip="Show only changed nodes"
        selected={
          graphSettings.ui_settings?.show_changed_nodes_only ===
          "WhenRightGraphPresent"
        }
        onSelectedChange={(selected) => {
          setGraphSettings({
            ...graphSettings,
            ui_settings: {
              ...graphSettings.ui_settings,
              show_changed_nodes_only: selected
                ? "WhenRightGraphPresent"
                : undefined,
            },
          });
        }}
      >
        <FileDiff />
      </UToggleButton>
    </>
  );
}

function EnableSelfMetricToggle({ metricName }: { metricName: string }) {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const metricSettings =
    graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};
  const selected = metricSettings?.column_hide_self !== true;

  return (
    <UToggleButton
      key={`${metricName}`}
      size="sm"
      tooltip={`Show a column with the values of '${metricName}' metric`}
      selected={selected}
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
      {selected ? <ToggleRight /> : <ToggleLeft />}
    </UToggleButton>
  );
}

function EnableTransitiveMetricToggle({ metricName }: { metricName: string }) {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const metricSettings =
    graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};
  const selected =
    metricSettings?.column_show_transitive === "WhenEnabledGlobally";

  return (
    <UToggleButton
      key={`${metricName}`}
      size="sm"
      tooltip={`Show a column with transitive values of '${metricName}' metric`}
      selected={selected}
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
      <Network />
    </UToggleButton>
  );
}

function EnableDominatedMetricToggle({ metricName }: { metricName: string }) {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const metricSettings =
    graphSettings?.ui_settings?.columns?.metric_settings?.[metricName] ?? {};
  const selected = isEnabledForGraphStructure(
    graphSettings?.ui_settings?.graph_structure,
    metricSettings?.show_dominated,
  );

  return (
    <UToggleButton
      key={`${metricName}`}
      size="sm"
      tooltip={`Show a column with dominated values for '${metricName}' metric`}
      selected={selected}
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
                  show_dominated: selected ? "WhenEnabledGlobally" : "Never",
                },
              },
            },
          },
        });
      }}
    >
      <TreePalm />
    </UToggleButton>
  );
}

function ConjointTieredToggle() {
  const [graphSettings, setGraphSettings] = useGraphSettings();

  return (
    <UToggleButton
      size="sm"
      tooltip={`Show conjoint tiered metric columns`}
      selected={
        graphSettings?.ui_settings?.columns?.show_conjoint_tiered_metrics ===
        true
      }
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
              show_conjoint_tiered_metrics: selected,
            },
          },
        });
      }}
    >
      <CircleDollarSign />
    </UToggleButton>
  );
}

function DominatedTieredToggle() {
  const [graphSettings, setGraphSettings] = useGraphSettings();

  const selected =
    (graphSettings?.ui_settings?.columns?.hide_dominated_tiered_metrics ??
      false) === false;
  return (
    <UToggleButton
      size="sm"
      tooltip={`Show dominated metric columns`}
      selected={selected}
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
              hide_dominated_tiered_metrics: !selected,
            },
          },
        });
      }}
    >
      <TreePalm />
    </UToggleButton>
  );
}

function EnableTieredMetricsToggle() {
  const [graphSettings, setGraphSettings] = useGraphSettings();

  return (
    <UToggleButton
      size="sm"
      tooltip={`Show tiered metric columns`}
      selected={
        graphSettings?.ui_settings?.columns?.show_tiered_metrics === true
      }
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
              show_tiered_metrics: selected,
            },
          },
        });
      }}
    >
      <Layers />
    </UToggleButton>
  );
}

function SeparatorVertical() {
  return <div className="border-l border border-accent py-3 h-full w-0" />;
}

function SeparatorHorizontal() {
  return <div className="border-t border border-accent w-full" />;
}
