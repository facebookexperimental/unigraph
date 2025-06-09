// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Ellipsis } from "lucide-react";
import type { Arrow } from "u-be/unigraph_core/bindings/Arrow";
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
} from "../components/ui/dropdown-menu";
import { useTVC } from "../context/TraversalConfigContext";
import type NativeGraph from "../NativeGraph";
import { UDropdownMenu } from "../components/UDropdownMenu";
import { useNativeGraph } from "../context/NativeGraphContext";

export default function ContextMenuCell(props: {
  arrow: Arrow;
}) {
  const nativeGraph = useNativeGraph();
  return (
    <UDropdownMenu
      content={<Content arrow={props.arrow} nativeGraph={nativeGraph} />}
    >
      <Ellipsis className="cursor-pointer hover:bg-primary rounded transition-all p-1" />
    </UDropdownMenu>
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
                  include: false,
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
                include: false,
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
