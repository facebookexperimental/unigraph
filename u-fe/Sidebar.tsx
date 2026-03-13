// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Info, SlidersHorizontal, Waypoints, Wrench } from "lucide-react";
import type { SidebarPanel } from "./__generated__/ts/SidebarPanel";
import UTooltip from "./components/UTooltip";
import { Button } from "./components/ui/button";
import { useDebugMode } from "./context/DebugModeContext";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useTwinGraph } from "./context/NativeGraphContext";
import TraversalConfigInspector from "./sidebar_panels/TraversalConfigInspector";

export default function Sidebar({
  selectedPanelTab,
}: {
  selectedPanelTab: SidebarPanel;
}) {
  const [debugMode, setDebugMode] = useDebugMode();
  const tg = useTwinGraph();
  return (
    <div className="flex h-full flex-col px-2 bg-sidebar pt-4 pb-3 border-r justify-between">
      <div className="flex flex-col items-center gap-2">
        <TabSelector tabName="Simulation" selectedPanelTab={selectedPanelTab}>
          <Waypoints />
        </TabSelector>
        <TabSelector tabName="GraphInfo" selectedPanelTab={selectedPanelTab}>
          <Info />
        </TabSelector>
        <TabSelector
          tabName="TraversalConfigEditor"
          selectedPanelTab={selectedPanelTab}
        >
          <SlidersHorizontal />
        </TabSelector>
        {debugMode && <TraversalConfigInspector />}
      </div>
      <UTooltip tooltip="Toggle debug mode that shows additional info">
        <Button
          size="icon"
          className="cursor-pointer"
          variant={debugMode ? "outline" : "ghost"}
          onClick={() => {
            const c = console;
            // biome-ignore lint/complexity/useLiteralKeys: just making sure it doen't get minified out
            c["log"](tg);
            setDebugMode(!debugMode);
          }}
        >
          <Wrench color={debugMode ? undefined : "#a7a3a4"} />
        </Button>
      </UTooltip>
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
