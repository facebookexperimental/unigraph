// Copyright (c) Meta Platforms, Inc. and affiliates.

import { GitCompareArrows } from "lucide-react";
import UDialog from "../components/UDialog";
import { Button } from "../components/ui/button";
import { useTVC } from "../context/TraversalConfigContext";
import TvcDiffView from "./TvcDiffView";

/// Renders nothing outside compare mode — there is no left config to diff.
///
/// The dialog body is only mounted while open (Radix unmounts closed content),
/// so the diff is never computed for users who don't ask for it.
export default function TvcDiffDialog() {
  const { tvcL, tvcR } = useTVC();
  if (tvcL == null) return null;

  return (
    <UDialog
      title="Traversal config — Left ↔ Right"
      // `sm:` is needed to beat DialogContent's own `sm:max-w-lg` — an
      // unprefixed `max-w-*` loses to it at the `sm` breakpoint and up.
      className="flex h-[85vh] w-[80vw] max-w-[80vw] flex-col sm:max-w-[80vw]"
      trigger={
        <Button variant="outline" size="sm" className="w-full">
          <GitCompareArrows className="size-3.5" />
          Diff Left ↔ Right
        </Button>
      }
    >
      <div className="min-h-0 flex-1">
        <TvcDiffView left={tvcL} right={tvcR} />
      </div>
    </UDialog>
  );
}
