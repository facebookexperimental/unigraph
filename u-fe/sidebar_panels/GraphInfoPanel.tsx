import { Card } from "../components/ui/card";
import { Separator } from "../components/ui/separator";
import { useNativeGraph } from "../context/NativeGraphContext";
import formatNumber from "../lib/formatNumber";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

export default function GraphInfoPanel() {
  const stats = useNativeGraph().stats();

  return (
    <SidebarPanel>
      <SidebarPanelHeader>Graph Info</SidebarPanelHeader>
      <div className="flex flex-col gap-4 pt-4">
        <Card className="p-4">
          <div className="text-xl">Nodes</div>
          <div className="flex flex-wrap gap-2 justify-between">
            <DataCell
              label="Reachable"
              value={stats.num_all_nodes - stats.num_unreachable_nodes}
            />
            <DataCell label="Total" value={stats.num_all_nodes} />
            <DataCell label="Unreachable" value={stats.num_unreachable_nodes} />
          </div>
        </Card>
        <Card className="p-4">
          <div className="text-xl">Edges</div>

          <div className="flex flex-wrap gap-2 justify-between">
            <DataCell
              label="Included"
              value={stats.num_all_edges - stats.num_excluded_edges}
            />
            <DataCell label="Total" value={stats.num_all_edges} />
            <DataCell label="Excluded" value={stats.num_excluded_edges} />
            <Separator className="my-4" />
            <DataCell label="Directed" value={stats.num_directed_edges} />
            <DataCell label="Tagged" value={stats.num_tagged_edges} />
            <DataCell label="Dynamic" value={stats.num_dynamic_edges} />
          </div>
        </Card>
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
      <span className="text-xl tabular-nums">{formatNumber(value, 0, 0)}</span>
      <span className="text-xs text-primary font-medium">{label}</span>
    </div>
  );
}
