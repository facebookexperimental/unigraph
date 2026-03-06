// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
} from "../../components/ui/card";

interface TimelineResponse {
  timeline_id: string;
}

export default function Home() {
  const navigate = useNavigate();
  const [timelines, setTimelines] = useState<TimelineResponse[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch("/api/timelines")
      .then((r) => {
        if (r.status === 404) {
          // No storage configured, redirect to explorer
          navigate("/explorer", { replace: true });
          return null;
        }
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((data: TimelineResponse[] | null) => {
        if (data == null) return;
        if (data.length === 0) {
          navigate("/explorer", { replace: true });
          return;
        }
        setTimelines(data);
      })
      .catch((e: unknown) =>
        setError(e instanceof Error ? e.message : String(e)),
      );
  }, [navigate]);

  if (error != null) {
    return (
      <div className="p-8 text-red-500">Failed to load timelines: {error}</div>
    );
  }

  if (timelines == null) {
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
        {timelines.map((tl) => (
          <Card
            key={tl.timeline_id}
            className="cursor-pointer hover:border-primary/50 transition-colors"
            onClick={() => navigate(`/timelines/${tl.timeline_id}`)}
          >
            <CardHeader>
              <CardTitle>{tl.timeline_id}</CardTitle>
              <CardDescription>Timeline</CardDescription>
            </CardHeader>
          </Card>
        ))}
      </div>
    </div>
  );
}
