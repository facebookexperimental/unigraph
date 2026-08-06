// Copyright (c) Meta Platforms, Inc. and affiliates.

import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { usePortalContainer } from "../context/GlobalElementRefs";
import { cn } from "../lib/utils";
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
  popoverClassName,
  toggleDisabled,
}: {
  selected?: boolean;
  onSelectedChange?: (selected: boolean) => void;
  children: React.ReactNode;
  tooltip?: React.ReactNode;
  popoverContent: React.ReactNode;
  /** Override the popover's size. Defaults to `w-96 max-h-[80vh]`. */
  popoverClassName?: string;
  /**
   * Make the toggle half inert while leaving the popover half live — for a
   * toggle that needs something configured in the popover before it means
   * anything.
   *
   * Uses `aria-disabled` rather than `disabled`: a truly disabled button drops
   * pointer events, which would swallow the very tooltip that explains why it
   * can't be pressed.
   */
  toggleDisabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const container = usePortalContainer();
  const variant = selected ? "default" : "secondary";

  return (
    <div className="inline-flex">
      <UTooltip tooltip={tooltip}>
        <Button
          size="sm"
          // `focus-visible:z-10` so the focus ring paints over the adjacent
          // half instead of being clipped by it — the two halves overlap.
          className={cn(
            "rounded-r-none relative focus-visible:z-10",
            toggleDisabled ? "opacity-50 cursor-default" : "cursor-pointer",
          )}
          variant={variant}
          aria-disabled={toggleDisabled}
          onClick={() => {
            if (!toggleDisabled) {
              onSelectedChange?.(!selected);
            }
          }}
        >
          {children}
        </Button>
      </UTooltip>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button
            size="sm"
            className="cursor-pointer rounded-l-none border-l border-l-background/20 px-1 relative focus-visible:z-10"
            variant={variant}
            onClick={() => setOpen(!open)}
          >
            <ChevronDown className="size-3" />
          </Button>
        </PopoverTrigger>
        {open && popoverContent != null && (
          <PopoverPortal container={container?.current}>
            <PopoverContent
              className={cn(
                "mx-6 overflow-y-auto",
                popoverClassName ?? "w-96 max-h-[80vh]",
              )}
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
