import { HoverCardPortal } from "@radix-ui/react-hover-card";
import { usePortalContainer } from "./PortalContext";
import { useState } from "react";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "./ui/hover-card";

// Copyright (c) Meta Platforms, Inc. and affiliates.
export default function UHoverCard({
  children,
  content,
}: {
  children: React.ReactNode;
  content: React.ReactNode;
}) {
  const container = usePortalContainer();
  const [open, setOpen] = useState(false);

  return (
    <HoverCard openDelay={0} onOpenChange={() => setOpen(!open)} open={open}>
      <HoverCardTrigger className="cursor-pointer">{children}</HoverCardTrigger>
      {open && (
        <HoverCardPortal container={container?.current}>
          <HoverCardContent className="w-96">{content}</HoverCardContent>
        </HoverCardPortal>
      )}
    </HoverCard>
  );
}
