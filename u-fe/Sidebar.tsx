// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Info, TableProperties, Waypoints } from "lucide-react";
import { Button } from "./components/ui/button";
import type { SidebarPanel } from "u-be/unigraph_core/bindings/SidebarPanel";
import { useGraphSettings } from "./context/GraphSettingsContext";

export default function Sidebar({
  selectedPanelTab,
}: { selectedPanelTab: SidebarPanel }) {
  return (
    <div className="flex h-full flex-col items-center gap-2 py-4 px-2 bg-sidebar border-r">
      <TabSelector tabName="Simulation" selectedPanelTab={selectedPanelTab}>
        <Waypoints />
      </TabSelector>
      <TabSelector tabName="GraphInfo" selectedPanelTab={selectedPanelTab}>
        <Info />
      </TabSelector>
      <TabSelector
        tabName="ColumnsSettings"
        selectedPanelTab={selectedPanelTab}
      >
        <TableProperties />
      </TabSelector>
    </div>
  );
}

function TabSelector({
  tabName,
  selectedPanelTab,

  children,
}: {
  tabName: SidebarPanel;
  selectedPanelTab: SidebarPanel;
  children: React.ReactNode;
}) {
  const selected = selectedPanelTab === tabName;
  const [settings, setSettings] = useGraphSettings();

  return (
    <div className="flex flex-col gap-2">
      <Button
        size="icon"
        className="cursor-pointer"
        variant={selected ? "default" : "ghost"}
        onClick={() => {
          const selected = selectedPanelTab === tabName;
          if (selected) {
            setSettings({
              ...settings,
              ui_settings: {
                ...settings.ui_settings,
                selected_sidebar_panel: "None",
              },
            });
          } else {
            setSettings({
              ...settings,
              ui_settings: {
                ...settings.ui_settings,
                selected_sidebar_panel: tabName,
              },
            });
          }
        }}
      >
        {children}
      </Button>
    </div>
  );
}
