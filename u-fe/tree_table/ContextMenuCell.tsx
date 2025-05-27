// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Ellipsis } from "lucide-react";
import type { Arrow } from "u-be/unigraph_core/bindings/Arrow";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "../components/ui/dropdown-menu";
import { useState } from "react";
import { useTVC } from "../context/TraversalConfigContext";
import type NativeGraph from "../NativeGraph";

export default function ContextMenuCell(props: {
  arrow: Arrow;
  nativeGraph: NativeGraph;
}) {
  const [isOpen, setIsOpen] = useState(false);
  return (
    <DropdownMenu open={isOpen} onOpenChange={setIsOpen}>
      <DropdownMenuTrigger asChild>
        <Ellipsis className="cursor-pointer hover:bg-primary rounded transition-all p-1" />
      </DropdownMenuTrigger>
      {isOpen && (
        <Content arrow={props.arrow} nativeGraph={props.nativeGraph} />
      )}
    </DropdownMenu>
  );
}

function Content({
  arrow,
  nativeGraph,
}: { arrow: Arrow; nativeGraph: NativeGraph }) {
  const { tvc, setTvc } = useTVC();

  const disableForce = arrow.points_from === -1;

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
      <DropdownMenuItem
        disabled={disableForce}
        className="cursor-pointer"
        onSelect={() => {
          const points_from: string = nativeGraph.getNodeName(
            arrow.points_from,
          );
          const points_to: string = nativeGraph.getNodeName(arrow.points_to);
          setTvc({
            ...tvc,
            force_edges: {
              ...tvc.force_edges,
              [points_from]: {
                ...(tvc.force_edges[points_from] ?? null),
                [points_to]: {
                  follow: false,
                  message:
                    "This edge was manually forced from the dropdown menu",
                },
              },
            },
          });
        }}
      >
        Exclude Edge
        <DropdownMenuShortcut>E</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem
        disabled={disableForce}
        className="cursor-pointer"
        onSelect={() => {
          const points_to: string = nativeGraph.getNodeName(arrow.points_to);
          setTvc({
            ...tvc,
            force_nodes: {
              ...tvc.force_nodes,
              [points_to]: {
                follow: false,
                message: "This node was manually forced from the dropdown menu",
              },
            },
          });
        }}
      >
        Exclude Node
        <DropdownMenuShortcut>N</DropdownMenuShortcut>
      </DropdownMenuItem>
    </DropdownMenuContent>
  );
}
