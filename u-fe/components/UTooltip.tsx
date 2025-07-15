// Copyright (c) Meta Platforms, Inc. and affiliates.

import { TooltipPortal } from "@radix-ui/react-tooltip";
import { usePortalContainer } from "./PortalContext";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export default function UTooltip({
  tooltip,
  children,
}: {
  tooltip: React.ReactNode | null;
  children: React.ReactNode;
}) {
  const container = usePortalContainer();

  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      {tooltip != null && (
        <TooltipPortal container={container?.current}>
          <TooltipContent>{tooltip}</TooltipContent>
        </TooltipPortal>
      )}
    </Tooltip>
  );
}
