// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback } from "react";
import { useSelectedPath } from "./context/SelectedPathContext";
import {
  useFlipForceEdge,
  useFlipForceExcludeNode,
} from "./context/TraversalConfigContext";

export function useExplorerKeyboardShortcuts(): (
  e: React.KeyboardEvent<HTMLDivElement>,
) => void {
  const { selectedRow } = useSelectedPath();
  const arrow = selectedRow?.arrow || null;

  const flipForceEdge = useFlipForceEdge(arrow);
  const flipForceExcludeNode = useFlipForceExcludeNode(arrow);

  const keyboardEventHandler = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      const key = e.key.toLowerCase();
      switch (key) {
        case "n": {
          if (flipForceExcludeNode.enabled) {
            flipForceExcludeNode.forceExcludeNode();
          }
          break;
        }
        case "e": {
          if (flipForceEdge.enabled) {
            flipForceEdge.forceEdge();
          }
          break;
        }
      }
    },
    [flipForceEdge, flipForceExcludeNode],
  );

  return keyboardEventHandler;
}
