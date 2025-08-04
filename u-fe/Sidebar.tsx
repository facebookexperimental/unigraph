// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Info, Waypoints } from "lucide-react";
import { IS_DEBUG_MODE } from "./DebugMode";
import type { SidebarPanel } from "./__generated__/ts/SidebarPanel";
import { Button } from "./components/ui/button";
import { useGraphSettings } from "./context/GraphSettingsContext";
import TraversalConfigInspector from "./sidebar_panels/TraversalConfigInspector";

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
      {IS_DEBUG_MODE && <TraversalConfigInspector />}
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
  );
}
