// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useState } from "react";
import {
  DropdownMenu,
  DropdownMenuPortal,
  DropdownMenuTrigger,
} from "./ui/dropdown-menu";
import { usePortalContainer } from "./PortalContext";

export function UDropdownMenu({
  children,
  content,
}: {
  children: React.ReactNode;
  className?: string;
  content: React.ReactNode;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const container = usePortalContainer();
  return (
    <DropdownMenu open={isOpen} onOpenChange={setIsOpen}>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      {isOpen && (
        <DropdownMenuPortal container={container?.current}>
          {content}
        </DropdownMenuPortal>
      )}
    </DropdownMenu>
  );
}
