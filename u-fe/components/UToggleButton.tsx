// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import UTooltip from "./UTooltip";
import { Button } from "./ui/button";

export default function UToggleButton({
  selected,
  onSelectedChange,
  children,
  tooltip,
  tooltipDelayDuration = 400,
  size = "icon",
  className = "",
}: {
  selected?: boolean;
  onSelectedChange?: (selected: boolean) => void;
  children: React.ReactNode;
  tooltip?: React.ReactNode;
  tooltipDelayDuration?: number;
  size?: "default" | "sm" | "lg" | "icon";
  className?: string;
}) {
  return (
    <UTooltip tooltip={tooltip} delayDuration={tooltipDelayDuration}>
      <Button
        size={size}
        className={clsx("cursor-pointer", className)}
        variant={selected ? "default" : "secondary"}
        onClick={() => {
          if (onSelectedChange) {
            onSelectedChange(!selected);
          }
        }}
      >
        {children}
      </Button>
    </UTooltip>
  );
}
