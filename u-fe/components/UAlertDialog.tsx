// Copyright (c) Meta Platforms, Inc. and affiliates.

import { usePortalContainer } from "./PortalContext";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogPortal,
} from "./ui/alert-dialog";

export default function UAlertDialog({
  children,
  open = false,
  defaultOpen = false,
  className = "",
}: {
  children: React.ReactNode;
  open?: boolean;
  defaultOpen?: boolean;
  className?: string;
}) {
  const container = usePortalContainer();
  return (
    <AlertDialog defaultOpen={defaultOpen} open={open}>
      <AlertDialogPortal container={container?.current}>
        <AlertDialogContent className={className}>
          {children}
        </AlertDialogContent>
      </AlertDialogPortal>
    </AlertDialog>
  );
}
