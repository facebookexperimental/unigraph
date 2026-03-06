// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";

interface FrameResponse {
  graph_id: number;
  timestamp: string;
  frame_type: string;
  base: number | null;
}

function FrameTypeBadge({ frameType }: { frameType: string }) {
  switch (frameType) {
    case "Full":
      return <Badge>Full</Badge>;
    case "Delta":
      return <Badge variant="secondary">Delta</Badge>;
    case "Error":
      return <Badge variant="destructive">Error</Badge>;
    case "Empty":
      return <Badge variant="outline">Empty</Badge>;
    default:
      return <Badge variant="outline">{frameType}</Badge>;
  }
}

function formatTimestamp(iso: string): string {
  const date = new Date(iso);
  return date.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export default function TimelinePage() {
  const { timelineId } = useParams();
  const navigate = useNavigate();
  const [frames, setFrames] = useState<FrameResponse[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch(`/api/timelines/${timelineId}/frames`)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((data: FrameResponse[]) => setFrames(data))
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : String(e)),
      );
  }, [timelineId]);

  if (error != null) {
    return (
      <div className="p-8 text-red-500">Failed to load frames: {error}</div>
    );
  }

  if (frames == null) {
    return (
      <div className="h-screen flex items-center justify-center">
        Loading...
      </div>
    );
  }

  return (
    <div className="p-8 max-w-5xl mx-auto">
      <div className="flex items-center gap-4 mb-6">
        <Button variant="ghost" size="sm" onClick={() => navigate("/")}>
          &larr; Back
        </Button>
        <h1 className="text-2xl font-bold">{timelineId}</h1>
        <span className="text-muted-foreground text-sm">
          {frames.length} frames
        </span>
      </div>

      <table className="w-full text-sm">
        <thead>
          <tr className="border-b text-left text-muted-foreground">
            <th className="py-2 pr-4">Graph ID</th>
            <th className="py-2 pr-4">Timestamp</th>
            <th className="py-2 pr-4">Type</th>
            <th className="py-2 pr-4">Base</th>
            <th className="py-2" />
          </tr>
        </thead>
        <tbody>
          {frames.map((frame) => {
            const canExplore =
              frame.frame_type === "Full" || frame.frame_type === "Delta";
            return (
              <tr key={frame.graph_id} className="border-b">
                <td className="py-2 pr-4 font-mono">{frame.graph_id}</td>
                <td className="py-2 pr-4">
                  {formatTimestamp(frame.timestamp)}
                </td>
                <td className="py-2 pr-4">
                  <FrameTypeBadge frameType={frame.frame_type} />
                </td>
                <td className="py-2 pr-4 font-mono text-muted-foreground">
                  {frame.base != null ? frame.base : "\u2014"}
                </td>
                <td className="py-2">
                  {canExplore && (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() =>
                        navigate(`/explorer/${timelineId}/${frame.graph_id}`)
                      }
                    >
                      Explore
                    </Button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
