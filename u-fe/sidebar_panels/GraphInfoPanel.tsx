import Metric from "../components/Metric";
import { Card } from "../components/ui/card";
import { Separator } from "../components/ui/separator";
import { useNativeGraphL } from "../context/NativeGraphContext";
import formatNumber from "../lib/formatNumber";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

export default function GraphInfoPanel() {
  const stats = useNativeGraphL().stats();

  return (
    <SidebarPanel>
      <SidebarPanelHeader text="Graph Info" />
      <div className="flex flex-col gap-4 pt-4">
        <Card className="p-4">
          <div className="text-xl">Nodes</div>
          <div className="flex flex-wrap gap-2 justify-between">
            <InfoMetric
              label="Reachable"
              value={stats.num_all_nodes - stats.num_unreachable_nodes}
            />
            <InfoMetric label="Total" value={stats.num_all_nodes} />
            <InfoMetric
              label="Unreachable"
              value={stats.num_unreachable_nodes}
            />
          </div>
        </Card>
        <Card className="p-4">
          <div className="text-xl">Edges</div>

          <div className="flex flex-wrap gap-2 justify-between">
            <InfoMetric
              label="Included"
              value={stats.num_all_edges - stats.num_excluded_edges}
            />
            <InfoMetric label="Total" value={stats.num_all_edges} />
            <InfoMetric label="Excluded" value={stats.num_excluded_edges} />
            <Separator className="my-4" />
            <InfoMetric label="Directed" value={stats.num_directed_edges} />
            <InfoMetric label="Tagged" value={stats.num_tagged_edges} />
            <InfoMetric label="Dynamic" value={stats.num_dynamic_edges} />
          </div>
        </Card>
      </div>
    </SidebarPanel>
  );
}

function InfoMetric({
  label,
  value,
}: {
  label: string;
  value: number;
}) {
  return <Metric value={formatNumber(value, 0, 0)} label={label} />;
}
