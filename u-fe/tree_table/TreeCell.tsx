// Copyright (c) Meta Platforms, Inc. and affiliates.

import { ChevronDown, ChevronRight, Dot, RefreshCw } from "lucide-react";
import type { Row } from "./TreeTableRows";
import { Badge } from "../components/ui/badge";
import clsx from "clsx";

export default function TreeCell(props: {
  row: Row;
  canExpand: boolean;
  onToggleExpand: (expanded: boolean) => void;
  minDepth: number;
  nodeName: string;
}) {
  const chevron = (() => {
    if (props.canExpand) {
      if (props.row.isCycle) {
        return <RefreshCw size={16} className="mx-2" />;
      }
      return props.row.expanded ? (
        <ChevronDown
          size={16}
          className="mx-2"
          onClick={() => {
            props.onToggleExpand(false);
          }}
        />
      ) : (
        <ChevronRight
          size={16}
          className="mx-2"
          onClick={() => {
            props.onToggleExpand(true);
          }}
        />
      );
    }
    return <span className="mx-2 w-4" />;
  })();

  const padding = [];
  for (let i = Math.max(props.minDepth - 1, 0); i < props.row.depth; i++) {
    padding.push(<Dot key={i} size={16} className="mx-2" />);
  }

  const badge = (() => {
    const tag = props.row.arrow.tag;
    const branch = props.row.arrow.branch;
    if (tag != null) {
      return (
        <Badge className="me-2 bg-green-800 text-xs py-0 px-0.5">{tag}</Badge>
      );
    } else if (branch != null) {
      const label = props.row.arrow.properties?.type ?? branch;
      return (
        <Badge className="me-2 bg-orange-800 text-xs py-0 px-0.5">
          {label}
        </Badge>
      );
    }

    return null;
  })();

  return (
    <div className="flex items-center">
      {padding}
      {chevron}
      {badge}
      <p
        className={clsx(
          "pe-4",
          props.row.arrow.excluded && "line-through text-foreground/50",
        )}
      >
        {props.nodeName}
      </p>
    </div>
  );
}
