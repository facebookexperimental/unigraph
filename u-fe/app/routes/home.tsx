// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
} from "../../components/ui/card";
import { rpc } from "../../api/rpc";

export default function Home() {
  const navigate = useNavigate();
  const [timelineIds, setTimelineIds] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    rpc("ListTimelines", {})
      .then((data) => {
        if (data.timeline_ids.length === 0) {
          navigate("/explorer/local", { replace: true });
          return;
        }
        setTimelineIds(data.timeline_ids);
      })
      .catch((e: unknown) => {
        if (e instanceof Error && e.message.includes("HTTP 404")) {
          navigate("/explorer/local", { replace: true });
          return;
        }
        setError(e instanceof Error ? e.message : String(e));
      });
  }, [navigate]);

  if (error != null) {
    return (
      <div className="p-8 text-red-500">Failed to load timelines: {error}</div>
    );
  }

  if (timelineIds == null) {
    return (
      <div className="h-screen flex items-center justify-center">
        Loading...
      </div>
    );
  }

  return (
    <div className="p-8 max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold mb-6">Timelines</h1>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
        {timelineIds.map((id) => (
          <Card
            key={id}
            className="cursor-pointer hover:border-primary/50 transition-colors"
            onClick={() => navigate(`/timelines/${id}`)}
          >
            <CardHeader>
              <CardTitle>{id}</CardTitle>
              <CardDescription>Timeline</CardDescription>
            </CardHeader>
          </Card>
        ))}
      </div>
    </div>
  );
}
