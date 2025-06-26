// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

export default function UTooltip({
  tooltip,
  children,
}: {
  tooltip: string;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent>{tooltip}</TooltipContent>
    </Tooltip>
  );
}
