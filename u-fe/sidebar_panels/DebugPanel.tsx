// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useMemo, useState } from "react";
import { Button } from "../components/ui/button";
import { useMetricViewState } from "../context/MetricViewStateContext";
import { useTVC } from "../context/TraversalConfigContext";
import { Pre } from "../Typography";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

type Tab = "tvc" | "available" | "visible";

export default function DebugPanel() {
  const [activeTab, setActiveTab] = useState<Tab>("tvc");

  return (
    <SidebarPanel>
      <SidebarPanelHeader text="Debug" />
      <TabBar activeTab={activeTab} setActiveTab={setActiveTab} />
      <div className="mt-4">
        {activeTab === "tvc" && <TVCTab />}
        {activeTab === "available" && <AvailableMetricsTab />}
        {activeTab === "visible" && <VisibleMetricsTab />}
      </div>
    </SidebarPanel>
  );
}

function TabBar({
  activeTab,
  setActiveTab,
}: {
  activeTab: Tab;
  setActiveTab: (tab: Tab) => void;
}) {
  const tabs: { id: Tab; label: string }[] = [
    { id: "tvc", label: "TVC" },
    { id: "available", label: "Available Metrics" },
    { id: "visible", label: "Visible Metrics" },
  ];

  return (
    <div className="flex gap-1">
      {tabs.map((tab) => (
        <Button
          key={tab.id}
          variant={activeTab === tab.id ? "default" : "ghost"}
          size="sm"
          className="cursor-pointer"
          onClick={() => setActiveTab(tab.id)}
        >
          {tab.label}
        </Button>
      ))}
    </div>
  );
}

function TVCTab() {
  const { tvcR } = useTVC();

  const json = useMemo(() => JSON.stringify(tvcR, null, 2), [tvcR]);

  return <Pre text={json} className="text-xs" />;
}

function AvailableMetricsTab() {
  const { availableViews } = useMetricViewState();

  return (
    <MetricList
      label={`${availableViews.length} available`}
      metrics={availableViews}
    />
  );
}

function VisibleMetricsTab() {
  const { visibleViews } = useMetricViewState();

  const sorted = useMemo(() => [...visibleViews].sort(), [visibleViews]);

  return <MetricList label={`${sorted.length} visible`} metrics={sorted} />;
}

function MetricList({ label, metrics }: { label: string; metrics: string[] }) {
  return (
    <div className="flex flex-col gap-1">
      <div className="text-xs text-muted-foreground">{label}</div>
      {metrics.map((m) => (
        <div
          key={m}
          className="text-sm font-mono bg-secondary rounded px-2 py-1"
        >
          {m}
        </div>
      ))}
    </div>
  );
}
