// Copyright (c) Meta Platforms, Inc. and affiliates.
import CopyToClipboard from "../components/CopyToClipboard";
import Metric from "../components/Metric";
import { Card } from "../components/ui/card";
import { Separator } from "../components/ui/separator";
import { useNativeGraphs } from "../context/NativeGraphContext";
import { useResolvedSource } from "../context/ResolvedSourceContext";
import type { ResolvedGraphRef } from "../Explorer";
import formatNumber from "../lib/formatNumber";
import type NativeGraph from "../native/NativeGraph";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

export default function GraphInfoPanel() {
  const [nativeGraphL, nativeGraphR] = useNativeGraphs();
  const resolvedSource = useResolvedSource();

  const labelR = nativeGraphL == null ? "" : " (Right)";
  const labelL = " (Left)";

  return (
    <SidebarPanel storageKey="graph-info">
      <div className="flex flex-col gap-8">
        <StatsForNativeGraph
          nativeGraph={nativeGraphR}
          label={labelR}
          graphRef={resolvedSource.right}
        />
        {nativeGraphL != null && (
          <StatsForNativeGraph
            nativeGraph={nativeGraphL}
            label={labelL}
            graphRef={resolvedSource.left}
          />
        )}
      </div>
    </SidebarPanel>
  );
}

function StatsForNativeGraph({
  nativeGraph,
  label,
  graphRef,
}: {
  nativeGraph: NativeGraph;
  label: string;
  graphRef: ResolvedGraphRef | undefined;
}) {
  const stats = nativeGraph.stats();
  return (
    <div>
      <SidebarPanelHeader text={`Graph Info${label}`} />
      <div className="flex flex-col gap-4 pt-4">
        {graphRef != null && <SourceLine graphRef={graphRef} />}
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
    </div>
  );
}

function InfoMetric({ label, value }: { label: string; value: number }) {
  return <Metric value={formatNumber(value, 0, 0)} label={label} />;
}

/**
 * Which concrete graph this side is showing, as `{timeline}~{graph_id}`. Worth
 * surfacing because the requested handle is often anonymous (`gqc_1a2b…`) or
 * floating (a bare timeline = latest), so neither ID is known until the fetch
 * comes back.
 */
function SourceLine({ graphRef }: { graphRef: ResolvedGraphRef }) {
  return (
    <div className="flex items-center gap-1 text-sm text-muted-foreground">
      <span className="font-mono truncate" title={graphRef.graph_key}>
        {graphRef.graph_key}
      </span>
      <CopyToClipboard text={graphRef.graph_key} />
    </div>
  );
}
