// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import { Button } from "./ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

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
  tooltip?: string;
  size?: "default" | "sm" | "lg" | "icon";
  className?: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
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
      </TooltipTrigger>
      {tooltip != null && <TooltipContent>{tooltip}</TooltipContent>}
    </Tooltip>
  );
}
