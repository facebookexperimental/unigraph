// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Ellipsis } from "lucide-react";
import type { Arrow } from "u-be/unigraph_core/bindings/Arrow";
import { canArrowBeForced, canNodeBeForceExcluded } from "../ArrowUtils";
import { UDropdownMenu } from "../components/UDropdownMenu";
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
} from "../components/ui/dropdown-menu";
import { useSelectedPath } from "../context/SelectedPathContext";
import {
  useForceEdge,
  useForceExcludeNode,
} from "../context/TraversalConfigContext";
import { type Row, pathToRow } from "./TreeTableRows";

export default function ContextMenuCell(props: {
  arrow: Arrow;
  row: Row;
}) {
  return (
    <UDropdownMenu content={<Content arrow={props.arrow} row={props.row} />}>
      <Ellipsis className="cursor-pointer hover:bg-primary rounded transition-all p-1" />
    </UDropdownMenu>
  );
}

function Content({ arrow, row }: { arrow: Arrow; row: Readonly<Row> }) {
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
    </DropdownMenuContent>
  );
}

function ExcludeNodeItem({ arrow, row }: { arrow: Arrow; row: Readonly<Row> }) {
  const forceExcludeNode = useForceExcludeNode();
  const { setSelectedPath } = useSelectedPath();

  const enabled = canNodeBeForceExcluded(arrow);

  return (
    <DropdownMenuItem
      disabled={!enabled}
      className="cursor-pointer"
      onSelect={() => {
        forceExcludeNode(arrow.points_to, !arrow.excluded);
        setSelectedPath(pathToRow(row), true);
      }}
    >
      {`${arrow.excluded ? "Undo Exclude" : "Exclude"} Node`}
      <DropdownMenuShortcut>N</DropdownMenuShortcut>
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
  const [isForcedTo, forceEdge] = useForceEdge(
    arrow.points_from,
    arrow.points_to,
  );

  const action: "Include" | "Exclude" = (() => {
    if (isForcedTo === null) {
      return arrow.excluded ? "Include" : "Exclude";
    }
    return isForcedTo ? "Exclude" : "Include";
  })();

  const canEdgeBeForced = canArrowBeForced(arrow);
  return (
    <DropdownMenuItem
      disabled={!canEdgeBeForced}
      className="cursor-pointer"
      onSelect={() => {
        forceEdge(action === "Include");
        setSelectedPath(pathToRow(row), true);
      }}
    >
      {`Force ${action} Edge`}
      <DropdownMenuShortcut>E</DropdownMenuShortcut>
    </DropdownMenuItem>
  );
}
