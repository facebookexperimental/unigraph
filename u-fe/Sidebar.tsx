// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Info, TableProperties, Waypoints } from "lucide-react";
import { type PanelTab, usePageParams } from "./PageParams";
import { Button } from "./components/ui/button";

export default function Sidebar({
  selectedPanelTab,
}: { selectedPanelTab: PanelTab }) {
  return (
    <div className="flex h-full flex-col items-center gap-2 py-4 px-2 bg-sidebar border-r">
      <TabSelector tabName="Simulation" selectedPanelTab={selectedPanelTab}>
        <Waypoints />
      </TabSelector>
      <TabSelector tabName="GraphInfo" selectedPanelTab={selectedPanelTab}>
        <Info />
      </TabSelector>
      <TabSelector tabName="Columns" selectedPanelTab={selectedPanelTab}>
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
  tabName: PanelTab;
  selectedPanelTab: PanelTab;
  children: React.ReactNode;
}) {
  const selected = selectedPanelTab === tabName;
  const [_pageParams, setPageParams] = usePageParams();

  return (
    <div className="flex flex-col gap-2">
      <Button
        size="icon"
        className="cursor-pointer"
        variant={selected ? "default" : "ghost"}
        onClick={() => {
          const selected = selectedPanelTab === tabName;
          if (selected) {
            setPageParams({ panelTab: "None" });
          } else {
            setPageParams({ panelTab: tabName });
          }
        }}
      >
        {children}
      </Button>
    </div>
  );
}
