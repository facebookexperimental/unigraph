import { useNativeGraph } from "../context/NativeGraphContext";
import { Separator } from "../components/ui/separator";
import formatNumber from "../lib/formatNumber";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

export default function GraphInfoPanel() {
  const stats = useNativeGraph().stats();

  return (
    <SidebarPanel>
      <SidebarPanelHeader>Graph Info</SidebarPanelHeader>
      <Separator className="mb-4" />
      <div className="flex flex-wrap gap-4 justify-around">
        <DataCell label="Directed Edges" value={stats.num_directed_edges} />
        <DataCell label="Tagged Edges" value={stats.num_tagged_edges} />
        <DataCell label="Dynamic Edges" value={stats.num_dynamic_edges} />
        <DataCell label="Node Count" value={stats.num_all_nodes} />
        <DataCell label="All Edges" value={stats.num_all_edges} />
      </div>
    </SidebarPanel>
  );
}

function DataCell({
  label,
  value,
}: {
  label: string;
  value: number;
}) {
  return (
    <div className="flex flex-col items-center">
      <span className="text-3xl">{formatNumber(value, 0)}</span>
      <span className="text-xs text-primary font-medium">{label}</span>
    </div>
  );
}
