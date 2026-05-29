// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  ArrowLeftRight,
  ArrowUpNarrowWide,
  ArrowUpToLine,
  ChartNoAxesCombined,
  FileDiff,
  List,
  Network,
  Tally5,
  ToggleLeft,
  ToggleRight,
  TreePalm,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { MetricViewVisibility } from "./__generated__/ts/MetricViewVisibility";
import { MetricsConfigResolver } from "./lib/MetricsConfigResolver";
import Metric from "./components/Metric";
import USplitToggleButton from "./components/USplitToggleButton";
import UToggleButton from "./components/UToggleButton";
import UTooltip from "./components/UTooltip";
import { Button } from "./components/ui/button";
import { useTreeTableRef } from "./context/GlobalElementRefs";
import {
  KEYBOARD_SHORTCUTS,
  KeyboardShortcutLabel,
} from "./context/GlobalKeyboardShortcutsContext";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useNativeGraphR, useTwinGraph } from "./context/NativeGraphContext";
import { useSelectedNodes } from "./context/SelectedNodesContext";
import { useTVC } from "./context/TraversalConfigContext";
import {
  useToggleDominatorTreeView,
  useToggleFlatListView,
  useToggleReverseView,
} from "./GraphStructureHooks";
import formatMetric from "./lib/formatMetric";
import formatNumber from "./lib/formatNumber";
import NodeSearch from "./NodeSearch";
import { H2, H3 } from "./Typography";
import { ENABLED, HIDDEN, MV } from "./tree_table/columns/ColumnUtils";

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
  const nativeGraph = useNativeGraphR();

  const resolver = useMemo(
    () => new MetricsConfigResolver(graphSettings),
    [graphSettings],
  );

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
      const format = resolver.format(metricName);
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
      const format = resolver.format(metricName);

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
  const nativeGraph = useNativeGraphR();
  const hasMetrics = nativeGraph.metricNames.length > 0;

  const [flatViewEnabled, toggleFlatListView] = useToggleFlatListView();
  const [reverseViewEnabled, toggleReverseView] = useToggleReverseView();
  const [dominatorTreeViewEnabled, toggleDominatorTreeView] =
    useToggleDominatorTreeView();

  return (
    <div className="flex gap-4 items-center">
      <USplitToggleButton
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
        popoverContent={<CountsHovercardContent />}
      >
        <Tally5 />
      </USplitToggleButton>

      {hasMetrics && (
        <USplitToggleButton
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
          popoverContent={<MetricsHovercardContent />}
        >
          <ChartNoAxesCombined />
        </USplitToggleButton>
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

// ── Helpers for setting per-view visibility ─────────────────────

function setViewVisibility(
  graphSettings: ReturnType<typeof useGraphSettings>[0],
  viewKey: string,
  visibility: MetricViewVisibility,
): ReturnType<typeof useGraphSettings>[0] {
  return new MetricsConfigResolver(graphSettings).setVisibility(
    graphSettings,
    viewKey,
    visibility,
  );
}

// ── Count toggles hovercard ─────────────────────────────────────

function CountsHovercardContent() {
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const twinGraph = useTwinGraph();
  const singleGraph = twinGraph.l == null;
  const resolver = new MetricsConfigResolver(graphSettings);
  const structure = graphSettings.ui_settings?.graph_structure ?? "Forward";

  // Set a count column's visibility and, when turning one on, also enable the
  // global "show counts" toggle so the column is actually rendered. Turning a
  // column off never flips the global toggle.
  const setCountVisibility = (
    viewKey: string,
    selected: boolean,
    visibilityWhenSelected: MetricViewVisibility,
  ) => {
    const updated = setViewVisibility(
      graphSettings,
      viewKey,
      selected ? visibilityWhenSelected : HIDDEN,
    );
    setGraphSettings({
      ...updated,
      ui_settings: {
        ...updated.ui_settings,
        columns: {
          ...updated.ui_settings?.columns,
          show_counts: selected
            ? true
            : updated.ui_settings?.columns?.show_counts,
        },
      },
    });
  };

  return (
    <div className="flex flex-col gap-2">
      <H3 text="Node count columns" />
      <div className="flex flex-wrap gap-2">
        <UToggleButton
          size="sm"
          tooltip="Show transitive children count"
          selected={resolver.isVisible(
            MV.countTransitive,
            "transitive",
            structure,
          )}
          onSelectedChange={(selected) => {
            setCountVisibility(MV.countTransitive, selected, ENABLED);
          }}
        >
          <Tally5 />
        </UToggleButton>

        {singleGraph && (
          <>
            <UToggleButton
              tooltip="Show number of parent nodes"
              size="sm"
              selected={resolver.isVisible(
                MV.parentsCount,
                "self_view",
                structure,
              )}
              onSelectedChange={(selected) => {
                setCountVisibility(MV.parentsCount, selected, ENABLED);
              }}
            >
              <ArrowUpNarrowWide />
            </UToggleButton>

            <UToggleButton
              tooltip="Show dominated nodes counts"
              size="sm"
              selected={resolver.isVisible(
                MV.countDominated,
                "dominated",
                structure,
              )}
              onSelectedChange={(selected) => {
                setCountVisibility(MV.countDominated, selected, ENABLED);
              }}
            >
              <TreePalm />
            </UToggleButton>
          </>
        )}
      </div>
    </div>
  );
}

function MetricsHovercardContent() {
  const twinGraph = useTwinGraph();
  const [graphSettings, setGraphSettings] = useGraphSettings();

  const hasTiers = twinGraph.r.stats().tier_names.length > 0;
  const metricCards = twinGraph.r.metricNames.map((metricName) => {
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
  const nativeGraph = useNativeGraphR();
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const { tvcL, setTvcL, tvcR, setTvcR } = useTVC();
  const allTiers = nativeGraph.stats().tier_names;
  const maxTiers = allTiers.map((tierName, tierIDX) => {
    const selected =
      tvcR.tiered_traversal?.AscendingTiers?.max_tier === tierIDX;

    return (
      <UToggleButton
        // biome-ignore lint/suspicious/noArrayIndexKey: because
        key={`tier-${tierIDX}`}
        size="sm"
        tooltip={`Only show nodes that are on or below '${tierName}'`}
        selected={selected}
        onSelectedChange={(selected) => {
          let newGraphSettings = { ...graphSettings };

          for (const metricName of nativeGraph.metricNames) {
            /// When max tier is selected we want to hide columns for all tiers above it, because
            /// their value will be 0 anyway and showing it will clutter the UI and make it confusing.
            for (let idx = 0; idx < allTiers.length; idx++) {
              const tn = allTiers[idx] as string;
              const vis = idx > tierIDX ? HIDDEN : ENABLED;
              // Set visibility for tiered views
              newGraphSettings = setViewVisibility(
                newGraphSettings,
                MV.tiered(metricName, tn),
                vis,
              );
            }
          }

          setGraphSettings(newGraphSettings);
          setTvcR({
            ...tvcR,
            tiered_traversal: {
              AscendingTiers: {
                tiers: tvcR.tiered_traversal?.AscendingTiers?.tiers ?? [],
                max_tier: selected ? tierIDX : undefined,
              },
            },
          });
          if (tvcL != null) {
            setTvcL({
              ...tvcL,
              tiered_traversal: {
                AscendingTiers: {
                  tiers: tvcL.tiered_traversal?.AscendingTiers?.tiers ?? [],
                  max_tier: selected ? tierIDX : undefined,
                },
              },
            });
          }
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

// A labeled group of metric toggle buttons. Renders nothing when it has no
// children so callers don't have to guard against empty sections themselves.
function MetricSection({
  label,
  icon,
  children,
}: {
  label: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
}) {
  const hasContent = Array.isArray(children)
    ? children.some(Boolean)
    : children != null && children !== false;

  if (!hasContent) {
    return null;
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        {icon}
        <span className="text-sm font-medium">{label}</span>
      </div>
      <div className="flex gap-2 flex-wrap">{children}</div>
    </div>
  );
}

function MetricCard({ metricName }: { metricName: string }) {
  const [graphSettings] = useGraphSettings();
  const nativeGraph = useNativeGraphR();
  const twinGraph = useTwinGraph();
  const singleGraph = twinGraph.l == null;
  const resolver = new MetricsConfigResolver(graphSettings);

  const allTiers = nativeGraph.stats().tier_names;
  const hasTiers = allTiers.length > 0;

  // Only show a toggle when its underlying view is actually available for this
  // metric. Dominated views additionally only make sense for a single graph.
  const selfAvailable = resolver.isAvailable(metricName, "self_view");
  const transitiveAvailable = resolver.isAvailable(metricName, "transitive");
  const dominatedAvailable =
    singleGraph && resolver.isAvailable(metricName, "dominated");
  const tieredAvailable =
    hasTiers && resolver.isAvailable(metricName, "tiered");
  const tieredDominatedAvailable =
    hasTiers &&
    singleGraph &&
    resolver.isAvailable(metricName, "tiered_dominated");

  const hasAnyView =
    selfAvailable ||
    transitiveAvailable ||
    dominatedAvailable ||
    tieredAvailable ||
    tieredDominatedAvailable;

  // Nothing is available for this metric — don't render an empty card.
  if (!hasAnyView) {
    return null;
  }

  return (
    <div className="w-full flex flex-col gap-3">
      <SeparatorHorizontal />
      <H2 text={metricName} />

      <MetricSection label="Node values">
        {selfAvailable && <EnableSelfMetricToggle metricName={metricName} />}
        {transitiveAvailable && (
          <EnableTransitiveMetricToggle metricName={metricName} />
        )}
        {dominatedAvailable && (
          <EnableDominatedMetricToggle metricName={metricName} />
        )}
      </MetricSection>

      {tieredAvailable && (
        <MetricSection
          label="Transitive by tier"
          icon={<Network className="size-4" />}
        >
          {allTiers.map((tierName) => (
            <ToggleTierForMetric
              key={`${metricName}-${tierName}`}
              tierName={tierName}
              metricName={metricName}
            />
          ))}
        </MetricSection>
      )}

      {tieredDominatedAvailable && (
        <MetricSection
          label="Dominated by tier"
          icon={<TreePalm className="size-4" />}
        >
          {allTiers.map((tierName) => (
            <DominatedForTierForMetric
              key={`${metricName}-dominated-${tierName}`}
              tierName={tierName}
              metricName={metricName}
            />
          ))}
        </MetricSection>
      )}
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
  const resolver = new MetricsConfigResolver(graphSettings);
  const viewKey = MV.tiered(metricName, tierName);
  const graphStructure =
    graphSettings.ui_settings?.graph_structure ?? "Forward";

  return (
    <UToggleButton
      key={`tiered-${metricName}-${tierName}`}
      size="sm"
      tooltip={`Show a column for transitive values of '${metricName}' metric for ${tierName} tier`}
      selected={resolver.isVisible(viewKey, "tiered", graphStructure)}
      onSelectedChange={(selected) => {
        setGraphSettings(
          setViewVisibility(
            graphSettings,
            viewKey,
            selected ? ENABLED : HIDDEN,
          ),
        );
      }}
    >
      <Network />
      <span className="text-sm">{tierName}</span>
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
  const resolver = new MetricsConfigResolver(graphSettings);
  const viewKey = MV.tieredDominated(metricName, tierName);
  const structure = graphSettings.ui_settings?.graph_structure ?? "Forward";

  const selected = resolver.isVisible(viewKey, "tiered_dominated", structure);

  return (
    <UToggleButton
      key={`dominated-tiered-${metricName}-${tierName}`}
      size="sm"
      tooltip={`Show a column for dominated values of '${metricName}' metric for ${tierName} tier`}
      selected={selected}
      onSelectedChange={(selected) => {
        const updated = setViewVisibility(
          graphSettings,
          viewKey,
          selected ? ENABLED : HIDDEN,
        );
        setGraphSettings({
          ...updated,
          ui_settings: {
            ...updated.ui_settings,
            columns: {
              ...updated.ui_settings?.columns,
              hide_metrics: false,
            },
          },
        });
      }}
    >
      <TreePalm />
      <span className="text-sm">{tierName}</span>
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

  if (twinGraph.l == null) {
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
  const resolver = new MetricsConfigResolver(graphSettings);
  const viewKey = MV.metric(metricName);
  const structure = graphSettings.ui_settings?.graph_structure ?? "Forward";
  const selected = resolver.isVisible(viewKey, "self_view", structure);

  return (
    <UToggleButton
      key={`${metricName}`}
      size="sm"
      tooltip={`Show a column with the values of '${metricName}' metric`}
      selected={selected}
      onSelectedChange={(selected) => {
        const updated = setViewVisibility(
          graphSettings,
          viewKey,
          selected ? ENABLED : HIDDEN,
        );
        setGraphSettings({
          ...updated,
          ui_settings: {
            ...updated.ui_settings,
            columns: {
              ...updated.ui_settings?.columns,
              hide_metrics: false,
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
  const resolver = new MetricsConfigResolver(graphSettings);
  const viewKey = MV.transitive(metricName);
  const structure = graphSettings.ui_settings?.graph_structure ?? "Forward";
  const selected = resolver.isVisible(viewKey, "transitive", structure);

  return (
    <UToggleButton
      key={`${metricName}`}
      size="sm"
      tooltip={`Show a column with transitive values of '${metricName}' metric`}
      selected={selected}
      onSelectedChange={(selected) => {
        const updated = setViewVisibility(
          graphSettings,
          viewKey,
          selected ? ENABLED : HIDDEN,
        );
        setGraphSettings({
          ...updated,
          ui_settings: {
            ...updated.ui_settings,
            columns: {
              ...updated.ui_settings?.columns,
              hide_metrics: false,
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
  const resolver = new MetricsConfigResolver(graphSettings);
  const viewKey = MV.dominated(metricName);
  const structure = graphSettings.ui_settings?.graph_structure ?? "Forward";
  const selected = resolver.isVisible(viewKey, "dominated", structure);

  return (
    <UToggleButton
      key={`${metricName}`}
      size="sm"
      tooltip={`Show a column with dominated values for '${metricName}' metric`}
      selected={selected}
      onSelectedChange={(selected) => {
        const updated = setViewVisibility(
          graphSettings,
          viewKey,
          selected ? ENABLED : HIDDEN,
        );
        setGraphSettings({
          ...updated,
          ui_settings: {
            ...updated.ui_settings,
            columns: {
              ...updated.ui_settings?.columns,
              hide_metrics: false,
            },
          },
        });
      }}
    >
      <TreePalm />
    </UToggleButton>
  );
}

function SeparatorVertical() {
  return <div className="border-l border border-accent py-3 h-full w-0" />;
}

function SeparatorHorizontal() {
  return <div className="border-t border border-accent w-full" />;
}
