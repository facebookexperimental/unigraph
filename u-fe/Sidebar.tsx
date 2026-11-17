// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Home, Wrench } from "lucide-react";
import type { ExplorerGraphSource, PanelTabPlugin } from "./Explorer";
import type { SidebarPanel } from "./__generated__/ts/SidebarPanel";
import UTooltip from "./components/UTooltip";
import { Button } from "./components/ui/button";
import { useDebugMode } from "./context/DebugModeContext";
import { useGraphSettings } from "./context/GraphSettingsContext";
import { useTwinGraph } from "./context/NativeGraphContext";
import CopyGqcKeyButton from "./sidebar_panels/CopyGqcKeyButton";

export default function Sidebar({
  selectedPanelTab,
  homeHref,
  panels,
  source,
}: {
  selectedPanelTab: string;
  homeHref?: string;
  panels: PanelTabPlugin[];
  source: ExplorerGraphSource;
}) {
  const [debugMode, setDebugMode] = useDebugMode();
  const tg = useTwinGraph();
  return (
    <div className="flex h-auto sm:h-full shrink-0 flex-row sm:flex-col p-2 bg-sidebar border-b sm:border-b-0 sm:border-r justify-between gap-2 overflow-x-auto">
      <div className="flex flex-row sm:flex-col items-center gap-2">
        {homeHref != null && (
          <UTooltip tooltip="Back to timelines">
            <Button className="cursor-pointer shrink-0" variant="ghost" asChild>
              <a href={homeHref} aria-label="Back to timelines">
                <Home />
                <span className="sm:hidden">Timelines</span>
              </a>
            </Button>
          </UTooltip>
        )}
        {panels.map((panel) => (
          <UTooltip key={panel.id} tooltip={panel.tooltip ?? panel.id}>
            <TabSelector
              tabName={panel.id}
              label={panel.tooltip ?? panel.id}
              selectedPanelTab={selectedPanelTab}
            >
              {panel.icon}
              <span className="sm:hidden">{panel.tooltip ?? panel.id}</span>
            </TabSelector>
          </UTooltip>
        ))}
        {debugMode && source.type === "handle" && (
          <CopyGqcKeyButton source={source} />
        )}
      </div>
      <UTooltip tooltip="Toggle debug mode that shows additional info">
        <Button
          aria-label="Debug mode"
          aria-pressed={debugMode}
          className="cursor-pointer shrink-0"
          variant={debugMode ? "outline" : "ghost"}
          onClick={() => {
            const c = console;
            // biome-ignore lint/complexity/useLiteralKeys: just making sure it doen't get minified out
            c["log"](tg);
            setDebugMode(!debugMode);
          }}
        >
          <Wrench color={debugMode ? undefined : "#a7a3a4"} />
          <span className="sm:hidden">Debug</span>
        </Button>
      </UTooltip>
    </div>
  );
}

function TabSelector({
  tabName,
  label,
  selectedPanelTab,
  children,
}: {
  tabName: string;
  label: string;
  selectedPanelTab: string;
  children: React.ReactNode;
}) {
  const selected = selectedPanelTab === tabName;
  const [settings, setSettings] = useGraphSettings();

  return (
    <Button
      aria-label={label}
      aria-pressed={selected}
      className="cursor-pointer shrink-0"
      variant={selected ? "default" : "ghost"}
      onClick={() => {
        setSettings({
          ...settings,
          ui_settings: {
            ...settings.ui_settings,
            selected_sidebar_panel: selected
              ? "None"
              : (tabName as SidebarPanel),
          },
        });
      }}
    >
      {children}
    </Button>
  );
}
