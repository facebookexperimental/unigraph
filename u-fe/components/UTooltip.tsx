// Copyright (c) Meta Platforms, Inc. and affiliates.

import { TooltipPortal } from "@radix-ui/react-tooltip";
import { usePortalContainer } from "./PortalContext";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export default function UTooltip({
  tooltip,
  children,
  delayDuration = 100,
}: {
  tooltip: React.ReactNode | null;
  children: React.ReactNode;
  delayDuration?: number;
}) {
  const container = usePortalContainer();

  return (
    <Tooltip delayDuration={delayDuration}>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      {tooltip != null && (
        <TooltipPortal container={container?.current}>
          <TooltipContent>{tooltip}</TooltipContent>
        </TooltipPortal>
      )}
    </Tooltip>
  );
}
