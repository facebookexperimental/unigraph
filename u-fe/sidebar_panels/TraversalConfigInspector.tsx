// Copyright (c) Meta Platforms, Inc. and affiliates.

import { GitGraph } from "lucide-react";
import { useMemo, useState } from "react";
import { Pre } from "../Typography";
import UAlertDialog from "../components/UAlertDialog";
import { AlertDialogTitle } from "../components/ui/alert-dialog";
import { Button } from "../components/ui/button";
import { useTVC } from "../context/TraversalConfigContext";

export default function TraversalConfigInspector() {
  const [isOpen, setIsOpen] = useState(false);
  const { tvc } = useTVC();

  const json = useMemo(() => {
    return JSON.stringify(tvc, null, 2);
  }, [tvc]);

  return (
    <>
      <Button
        size="icon"
        className="cursor-pointer"
        variant="ghost"
        onClick={() => setIsOpen(true)}
      >
        <GitGraph />
      </Button>
      {isOpen && (
        <UAlertDialog
          open={true}
          className="max-w-[1100px] max-h-[85vh] flex flex-col"
        >
          <div className="flex flex-col gap-2 w-full h-full min-w-0 overflow-auto">
            <AlertDialogTitle>Traversal Config</AlertDialogTitle>
            <Pre text={json} />
            <div className="flex justify-end my-4">
              <Button
                type="submit"
                className="cursor-pointer"
                onClick={() => setIsOpen(false)}
              >
                Close
              </Button>
            </div>
          </div>
        </UAlertDialog>
      )}
    </>
  );
}
