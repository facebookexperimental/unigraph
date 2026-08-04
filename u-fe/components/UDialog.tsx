// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useRef } from "react";
import { usePortalContainer } from "../context/GlobalElementRefs";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "./ui/dialog";

export default function UDialog({
  children,
  trigger,
  title,
  className,
}: {
  children: React.ReactNode;
  trigger: React.ReactNode;
  title: string;
  className?: string;
}) {
  const container = usePortalContainer();
  const contentRef = useRef<HTMLDivElement>(null);

  return (
    <Dialog>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent
        ref={contentRef}
        container={container?.current}
        className={className}
        // Radix focuses the first focusable descendant on open, which pops
        // open that control's tooltip. Focus the dialog itself instead — it
        // still traps focus and handles Esc, without the phantom tooltip.
        tabIndex={-1}
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          contentRef.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  );
}
