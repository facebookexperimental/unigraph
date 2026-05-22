import { HoverCardPortal } from "@radix-ui/react-hover-card";
import { useState } from "react";
import { usePortalContainer } from "../context/GlobalElementRefs";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "./ui/hover-card";

// Copyright (c) Meta Platforms, Inc. and affiliates.
export default function UHoverCard({
  children,
  content,
  asChild,
  triggerClassname,
  openDelay = 500,
}: {
  children: React.ReactNode;
  content: React.ReactNode;
  asChild?: boolean;
  triggerClassname?: string;
  openDelay?: number;
}) {
  const container = usePortalContainer();
  const [open, setOpen] = useState(false);

  return (
    <HoverCard
      openDelay={openDelay}
      onOpenChange={() => setOpen(!open)}
      open={open}
    >
      <HoverCardTrigger className={triggerClassname} asChild={asChild}>
        {children}
      </HoverCardTrigger>
      {open && content != null && (
        <HoverCardPortal container={container?.current}>
          <HoverCardContent className="w-96 mx-6">{content}</HoverCardContent>
        </HoverCardPortal>
      )}
    </HoverCard>
  );
}
