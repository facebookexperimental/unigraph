// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import UTooltip from "./UTooltip";
import { Button } from "./ui/button";

export default function UToggleButton({
  selected,
  onSelectedChange,
  children,
  tooltip,
  size = "icon",
  className = "",
}: {
  selected?: boolean;
  onSelectedChange?: (selected: boolean) => void;
  children: React.ReactNode;
  tooltip?: React.ReactNode;
  size?: "default" | "sm" | "lg" | "icon";
  className?: string;
}) {
  return (
    <UTooltip tooltip={tooltip}>
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
