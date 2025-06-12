import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import USwitch from "../components/USwitch";
import { Collapsible, CollapsibleTrigger } from "../components/ui/collapsible";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useNativeGraph } from "../context/NativeGraphContext";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

export default function ColumnsPanel() {
  const nativeGraph = useNativeGraph();
  const metricNames = nativeGraph.metricNames;
  const [graphSettings, setGraphSettings] = useGraphSettings();

  const cards = metricNames.map((metricName) => {
    return <MetricSettingsCard key={metricName} metricName={metricName} />;
  });

  return (
    <SidebarPanel>
      <SidebarPanelHeader>Columns</SidebarPanelHeader>
      <div className="mb-8 flex flex-col gap-4">
        <USwitch
          label="Show Parent Counts"
          checked={
            graphSettings.ui_settings?.columns?.show_parents_count === true
          }
          onCheckedChange={(checked) => {
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
        />
        <USwitch
          label="Show As A Flat List"
          checked={graphSettings.ui_settings?.show_as_a_flat_list === true}
          onCheckedChange={(checked) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                show_as_a_flat_list: checked,
              },
            });
          }}
        />
      </div>
      <SidebarPanelHeader>Metrics</SidebarPanelHeader>
      <div className="flex flex-col gap-2">{cards}</div>
    </SidebarPanel>
  );
}

function MetricSettingsCard({
  metricName,
}: {
  metricName: string;
}) {
  const [isOpen, setIsOpen] = useState(true);

  return (
    <Collapsible
      open={isOpen}
      onOpenChange={setIsOpen}
      className="flex w-full flex-col gap-2 bg-accent rounded-lg py-1"
    >
      <CollapsibleTrigger className="cursor-pointer flex justify-between mx-2 my-1">
        <p className="px-2">{metricName}</p>
        {isOpen ? <ChevronDown /> : <ChevronRight />}
      </CollapsibleTrigger>
      {isOpen && <ColumnCardContent metricName={metricName} />}
    </Collapsible>
  );
}

function ColumnCardContent({
  metricName,
}: {
  metricName: string;
}) {
  const nativeGraph = useNativeGraph();
  const [graphSettings, setGraphSettings] = useGraphSettings();
  const metricSettings = graphSettings.metric_settings?.[metricName] ?? {};

  const hidden = metricSettings.column_hide_trantitive_tiered ?? [];
  const transitiveSwitches = nativeGraph.stats().tier_names.map((tierName) => {
    const checked = !hidden.includes(tierName);
    return (
      <USwitch
        key={`${metricName}-${tierName}`}
        label={`Show ${tierName} transitive`}
        checked={checked}
        onCheckedChange={(checked) => {
          const column_hide_trantitive_tiered = checked
            ? hidden.filter((name) => name !== tierName)
            : [...hidden, tierName];

          setGraphSettings({
            ...graphSettings,
            metric_settings: {
              ...graphSettings.metric_settings,
              [metricName]: {
                ...metricSettings,
                column_hide_trantitive_tiered,
              },
            },
          });
        }}
      />
    );
  });

  return (
    <div className="flex flex-col gap-4 bg-sidebar mx-1 p-3 rounded-lg">
      <USwitch
        label="Show"
        checked={metricSettings.column_hide_self !== true}
        onCheckedChange={(checked) => {
          setGraphSettings({
            ...graphSettings,
            metric_settings: {
              ...graphSettings.metric_settings,
              [metricName]: {
                ...metricSettings,
                column_hide_self: !checked,
              },
            },
          });
        }}
      />
      <USwitch
        label="Show Transitive"
        checked={metricSettings.column_hide_transitive !== true}
        onCheckedChange={(checked) => {
          setGraphSettings({
            ...graphSettings,
            metric_settings: {
              ...graphSettings.metric_settings,
              [metricName]: {
                ...metricSettings,
                column_hide_transitive: !checked,
              },
            },
          });
        }}
      />

      {transitiveSwitches}
    </div>
  );
}
