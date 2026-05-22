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
    <div className="flex h-full flex-col px-2 bg-sidebar pt-4 pb-3 border-r justify-between">
      <div className="flex flex-col items-center gap-2">
        {homeHref != null && (
          <UTooltip tooltip="Back to timelines">
            <Button
              size="icon"
              className="cursor-pointer"
              variant="ghost"
              asChild
            >
              <a href={homeHref}>
                <Home />
              </a>
            </Button>
          </UTooltip>
        )}
        {panels.map((panel) => (
          <UTooltip key={panel.id} tooltip={panel.tooltip ?? panel.id}>
            <TabSelector tabName={panel.id} selectedPanelTab={selectedPanelTab}>
              {panel.icon}
            </TabSelector>
          </UTooltip>
        ))}
        {debugMode && source.type === "handle" && (
          <CopyGqcKeyButton source={source} />
        )}
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
  tabName: string;
  selectedPanelTab: string;
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
              selected_sidebar_panel: tabName as SidebarPanel,
            },
          });
        }
      }}
    >
      {children}
    </Button>
  );
}
