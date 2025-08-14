// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { Arrow } from "@/__generated__/ts/Arrow";
import { Ellipsis } from "lucide-react";
import {
  KEYBOARD_SHORTCUTS,
  KeyboardShortcutLabel,
} from "../ExplorerKeyboardShortcutsWrapper";
import { UDropdownMenu } from "../components/UDropdownMenu";
import {
  DropdownMenuContent,
  DropdownMenuItem,
} from "../components/ui/dropdown-menu";
import { useSelectedPath } from "../context/SelectedPathContext";
import {
  useFlipForceEdgeL,
  useFlipForceExcludeNodeL,
} from "../context/TraversalConfigContext";
import { type Row, pathToRow } from "./TreeTableRows";

export default function ContextMenuCell(props: {
  row: Row;
}) {
  return (
    <UDropdownMenu content={<Content row={props.row} />}>
      <Ellipsis className="cursor-pointer hover:bg-primary rounded transition-all p-1" />
    </UDropdownMenu>
  );
}

function Content({ row }: { row: Readonly<Row> }) {
  const arrowL = row.arrow_pair.l;

  if (arrowL == null) {
    // exclusion gets messy with two graphs. need to figure this one out later
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
      <ForceEdgeItem arrow={arrowL} row={row} />
      <ExcludeNodeItem arrow={arrowL} row={row} />
    </DropdownMenuContent>
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
      <KeyboardShortcutLabel
        label={KEYBOARD_SHORTCUTS.FORCE_EXCLUDE_NODE.toUpperCase()}
      />
    </DropdownMenuItem>
  );
}

function ForceEdgeItem({
  arrow,
  row,
}: {
  arrow: Arrow;
  row: Readonly<Row>;
}) {
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
      <KeyboardShortcutLabel
        label={KEYBOARD_SHORTCUTS.FORCE_EDGE.toUpperCase()}
      />
    </DropdownMenuItem>
  );
}
