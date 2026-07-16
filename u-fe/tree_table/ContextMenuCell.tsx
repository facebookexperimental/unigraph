// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Ellipsis } from "lucide-react";
import type { Arrow } from "@/__generated__/ts/Arrow";
import { UDropdownMenu } from "../components/UDropdownMenu";
import {
  DropdownMenuContent,
  DropdownMenuItem,
} from "../components/ui/dropdown-menu";
import {
  KEYBOARD_SHORTCUTS,
  KeyboardShortcutLabel,
} from "../context/GlobalKeyboardShortcutsContext";
import { useGraphSettings } from "../context/GraphSettingsContext";
import { useMinCut } from "../context/MinCutContext";
import { useNativeGraphR } from "../context/NativeGraphContext";
import { useSelectedPath } from "../context/SelectedPathContext";
import {
  useFlipForceEdgeL,
  useFlipForceExcludeNodeL,
} from "../context/TraversalConfigContext";
import { pathToRow, type Row } from "./TreeTableRows";

export default function ContextMenuCell(props: { row: Row }) {
  return (
    <UDropdownMenu content={<Content row={props.row} />}>
      <Ellipsis className="cursor-pointer hover:bg-primary rounded transition-all p-1" />
    </UDropdownMenu>
  );
}

function Content({ row }: { row: Readonly<Row> }) {
  // The context-menu column is only rendered in single-graph mode
  // (twinGraph.l === null), where the edge lives in the `r` arrow. The force
  // hooks below also operate on the right graph despite their `L` suffix.
  const arrow = row.twinArrow.r;

  if (arrow == null) {
    return null;
  }
  return (
    <DropdownMenuContent
      className="w-56"
      onMouseDown={(e) => {
        // prevent three table to select the row at click coordinates
        // when the context menu is open and clicked on.
        e.stopPropagation();
        e.preventDefault();
      }}
    >
      <ForceEdgeItem arrow={arrow} row={row} />
      <ExcludeNodeItem arrow={arrow} row={row} />
      <MinCutItem arrow={arrow} />
    </DropdownMenuContent>
  );
}

function MinCutItem({ arrow }: { arrow: Arrow }) {
  const { addSink } = useMinCut();
  const nativeGraph = useNativeGraphR();
  const [graphSettings, setGraphSettings] = useGraphSettings();

  return (
    <DropdownMenuItem
      className="cursor-pointer"
      onSelect={() => {
        const idx = arrow.points_to;
        addSink({ idx, name: nativeGraph.getNodeName(idx) });
        setGraphSettings({
          ...graphSettings,
          ui_settings: {
            ...graphSettings.ui_settings,
            selected_sidebar_panel: "MinCut",
          },
        });
      }}
    >
      Min Cut
    </DropdownMenuItem>
  );
}

function ExcludeNodeItem({ arrow, row }: { arrow: Arrow; row: Readonly<Row> }) {
  const { action, enabled, forceExcludeNode } = useFlipForceExcludeNodeL(arrow);
  const { setSelectedPath } = useSelectedPath();

  return (
    <DropdownMenuItem
      disabled={!enabled}
      className="cursor-pointer"
      onSelect={() => {
        forceExcludeNode();
        setSelectedPath(pathToRow(row), true);
      }}
    >
      {`${action === "Include" ? "Undo Exclude" : "Exclude"} Node`}
      <KeyboardShortcutLabel shortcut={KEYBOARD_SHORTCUTS.FORCE_EXCLUDE_NODE} />
    </DropdownMenuItem>
  );
}

function ForceEdgeItem({ arrow, row }: { arrow: Arrow; row: Readonly<Row> }) {
  const { setSelectedPath } = useSelectedPath();
  const { enabled, forceEdge, action } = useFlipForceEdgeL(arrow);

  return (
    <DropdownMenuItem
      disabled={!enabled}
      className="cursor-pointer"
      onSelect={() => {
        forceEdge();
        setSelectedPath(pathToRow(row), true);
      }}
    >
      {`Force ${action} Edge`}
      <KeyboardShortcutLabel shortcut={KEYBOARD_SHORTCUTS.FORCE_EDGE} />
    </DropdownMenuItem>
  );
}
