// Copyright (c) Meta Platforms, Inc. and affiliates.
import clsx from "clsx";

export default function Metric({
  label,
  value,
  metricSize = "text-xl",
}: {
  label: string;
  value: string;
  metricSize?: "text-sm" | "text-base" | "text-lg" | "text-xl"; // tailwind size classes
}) {
  return (
    <div className="flex flex-col items-center">
      <span className={clsx("tabular-nums font-mono", metricSize)}>
        {value}
      </span>
      <span className="text-xs text-muted-foreground font-medium">{label}</span>
    </div>
  );
}
