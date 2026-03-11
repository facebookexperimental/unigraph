// Copyright (c) Meta Platforms, Inc. and affiliates.

import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { usePortalContainer } from "../context/GlobalElementRefs";
import UTooltip from "./UTooltip";
import { Button } from "./ui/button";
import {
  Popover,
  PopoverContent,
  PopoverPortal,
  PopoverTrigger,
} from "./ui/popover";

export default function USplitToggleButton({
  selected,
  onSelectedChange,
  children,
  tooltip,
  popoverContent,
}: {
  selected?: boolean;
  onSelectedChange?: (selected: boolean) => void;
  children: React.ReactNode;
  tooltip?: React.ReactNode;
  popoverContent: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const container = usePortalContainer();
  const variant = selected ? "default" : "secondary";

  return (
    <div className="inline-flex">
      <UTooltip tooltip={tooltip}>
        <Button
          size="sm"
          className="cursor-pointer rounded-r-none"
          variant={variant}
          onClick={() => onSelectedChange?.(!selected)}
        >
          {children}
        </Button>
      </UTooltip>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button
            size="sm"
            className="cursor-pointer rounded-l-none border-l border-l-background/20 px-1"
            variant={variant}
            onClick={() => setOpen(!open)}
          >
            <ChevronDown className="size-3" />
          </Button>
        </PopoverTrigger>
        {open && popoverContent != null && (
          <PopoverPortal container={container?.current}>
            <PopoverContent
              className="w-96 mx-6"
              onOpenAutoFocus={(e) => e.preventDefault()}
            >
              {popoverContent}
            </PopoverContent>
          </PopoverPortal>
        )}
      </Popover>
    </div>
  );
}
