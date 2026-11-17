// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import UTooltip from "./UTooltip";
import { Button } from "./ui/button";

export default function UToggleButton({
  selected,
  onSelectedChange,
  children,
  tooltip,
  label,
  tooltipDelayDuration = 400,
  size = "icon",
  className = "",
}: {
  selected?: boolean;
  onSelectedChange?: (selected: boolean) => void;
  children: React.ReactNode;
  label?: string;
  tooltip?: React.ReactNode;
  tooltipDelayDuration?: number;
  size?: "default" | "sm" | "lg" | "icon";
  className?: string;
}) {
  return (
    <UTooltip tooltip={tooltip} delayDuration={tooltipDelayDuration}>
      <Button
        aria-pressed={selected === true}
        aria-label={
          label ?? (typeof tooltip === "string" ? tooltip : undefined)
        }
        size={size}
        className={clsx(
          "cursor-pointer",
          label != null && "w-auto shrink-0",
          className,
        )}
        variant={selected ? "default" : "secondary"}
        onClick={() => {
          if (onSelectedChange) {
            onSelectedChange(!selected);
          }
        }}
      >
        {children}
        {label != null && <span className="sm:hidden">{label}</span>}
      </Button>
    </UTooltip>
  );
}
