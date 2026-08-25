// Copyright (c) Meta Platforms, Inc. and affiliates.

/**
 * Side-by-side JSON diff. Generic — hand it any two values.
 *
 * Both panes live in ONE scroll container and ONE virtualizer: each row renders
 * both cells into a two-column grid. This is why the sides stay together. Two
 * independently scrolled panes would need scroll syncing and would drift apart
 * the moment a row height differed; sharing the row box makes alignment
 * structural rather than something to keep in sync.
 *
 * Row heights are fixed for the same reason — `getTotalSize()` stays exact, so
 * the scrollbar doesn't jump when a gap expands. Long values are truncated with
 * CSS rather than sliced, so the full text is still there for `title` and copy.
 *
 * Deliberately mirrors `tvc_diff/TvcDiffView`. The two views differ only in
 * what a row is; keeping the chrome identical means one place to learn.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronDown, ChevronUp, ChevronsUpDown } from "lucide-react";
import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Switch } from "../components/ui/switch";
import { cn } from "../lib/utils";
import type {
  GapRow,
  JsonDiffOpts,
  JsonDiffRow,
  JsonLine,
  LineRow,
  LineTone,
} from "./JsonDiffModel";
import {
  buildJsonDiff,
  expandJsonGap,
  findNextJsonChange,
  jsonDiffRowKey,
  searchJsonDiff,
} from "./JsonDiffModel";

const ROW_HEIGHT = 18;
const INDENT_PX = 12;

/// Expanding more than this at once is a visible hitch, so make it deliberate.
const EXPAND_ALL_WARN = 5000;

export default function UJSONDiff({
  left,
  right,
  leftLabel = "Left",
  rightLabel = "Right",
  identicalNote,
  opts,
  className,
}: {
  left: unknown;
  right: unknown;
  leftLabel?: string;
  rightLabel?: string;
  /// Shown alongside "identical on both sides". Two equal values can still sit
  /// under a row the surrounding view marks as changed, and only the caller
  /// knows why.
  identicalNote?: React.ReactNode;
  opts?: JsonDiffOpts;
  className?: string;
}) {
  const diff = useMemo(
    () => buildJsonDiff(left, right, opts),
    [left, right, opts],
  );
  const [rows, setRows] = useState<readonly JsonDiffRow[]>(diff.rows);
  const [query, setQuery] = useState("");
  const [changesOnly, setChangesOnly] = useState(false);
  const deferredQuery = useDeferredValue(query);
  const scrollRef = useRef<HTMLDivElement>(null);
  const cursor = useRef(-1);

  useEffect(() => setRows(diff.rows), [diff]);

  const visible = useMemo(() => {
    if (deferredQuery.trim() !== "") return searchJsonDiff(diff, deferredQuery);
    if (!changesOnly) return rows;
    return rows.filter((row) => row.kind === "line" && row.tone !== "context");
  }, [diff, rows, deferredQuery, changesOnly]);

  const virtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 16,
  });

  const expand = useCallback(
    (gap: GapRow, mode: "up" | "down" | "all") => {
      if (mode === "all" && gap.len > EXPAND_ALL_WARN) {
        const ok = window.confirm(
          `Expand all ${gap.len.toLocaleString()} unchanged lines? This may take a moment.`,
        );
        if (!ok) return;
      }
      setRows((prev) => {
        const at = prev.indexOf(gap);
        if (at < 0) return prev;
        const next = [...prev];
        next.splice(at, 1, ...expandJsonGap(diff, gap, mode));
        return next;
      });
    },
    [diff],
  );

  const jump = useCallback(
    (direction: 1 | -1) => {
      const from =
        cursor.current < 0 && direction === -1
          ? visible.length
          : cursor.current;
      const next = findNextJsonChange(visible, from, direction);
      if (next == null) return;
      cursor.current = next;
      virtualizer.scrollToIndex(next, { align: "center" });
    },
    [visible, virtualizer],
  );

  const identical =
    diff.counts.added === 0 &&
    diff.counts.removed === 0 &&
    diff.counts.changed === 0;

  return (
    <div className={cn("flex h-full min-h-0 flex-col gap-2", className)}>
      <Toolbar
        query={query}
        setQuery={setQuery}
        changesOnly={changesOnly}
        setChangesOnly={setChangesOnly}
        counts={diff.counts}
        onJump={jump}
      />
      {identical && (
        <div className="flex flex-col gap-1">
          <div className="text-muted-foreground text-xs italic">
            identical on both sides
          </div>
          {identicalNote}
        </div>
      )}
      <PaneHeader leftLabel={leftLabel} rightLabel={rightLabel} />
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto border-t">
        <div
          className="relative w-full"
          style={{ height: virtualizer.getTotalSize() }}
        >
          {virtualizer.getVirtualItems().map((item) => {
            const row = visible[item.index];
            if (row == null) return null;
            return (
              <div
                key={jsonDiffRowKey(row)}
                className="absolute top-0 left-0 w-full"
                style={{
                  height: item.size,
                  transform: `translateY(${item.start}px)`,
                }}
              >
                <Row row={row} onExpand={expand} />
              </div>
            );
          })}
        </div>
      </div>
      {visible.length === 0 && (
        <div className="text-muted-foreground p-4 text-center text-xs">
          No matching lines.
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

function Row({
  row,
  onExpand,
}: {
  row: JsonDiffRow;
  onExpand: (gap: GapRow, mode: "up" | "down" | "all") => void;
}) {
  switch (row.kind) {
    case "gap":
      return <Gap row={row} onExpand={onExpand} />;
    case "truncated":
      return (
        <div
          className="text-muted-foreground bg-muted/30 px-2 text-center font-mono text-[11px]"
          style={{ height: ROW_HEIGHT, lineHeight: `${ROW_HEIGHT}px` }}
        >
          stopped after {row.shown.toLocaleString()} lines — use search to find
          the rest
        </div>
      );
    case "line":
      return <Line row={row} />;
  }
}

function Line({ row }: { row: LineRow }) {
  const { tone, left, right } = row;
  return (
    <div
      className="grid grid-cols-2 font-mono text-[11px]"
      style={{ height: ROW_HEIGHT, lineHeight: `${ROW_HEIGHT}px` }}
    >
      <Cell
        line={left}
        tone={tone === "removed" || tone === "changed" ? "removed" : "context"}
      />
      <Cell
        line={right}
        tone={tone === "added" || tone === "changed" ? "added" : "context"}
      />
    </div>
  );
}

function Cell({
  line,
  tone,
}: {
  line: JsonLine | null;
  tone: "added" | "removed" | "context";
}) {
  if (line == null) {
    return <div className="bg-muted/20 border-border/50 h-full border-r" />;
  }

  const sign = tone === "added" ? "+" : tone === "removed" ? "-" : " ";

  return (
    <div
      title={line.text}
      className={cn(
        "border-border/50 flex h-full overflow-hidden border-r pr-2 whitespace-nowrap",
        tone === "added" && "bg-green-500/10",
        tone === "removed" && "bg-red-500/10",
      )}
    >
      <span
        className={cn(
          "w-4 shrink-0 text-center select-none",
          tone === "added" && "text-green-600 dark:text-green-400",
          tone === "removed" && "text-red-600 dark:text-red-400",
        )}
      >
        {sign}
      </span>
      <span
        className="overflow-hidden text-ellipsis"
        style={{ paddingLeft: line.indent * INDENT_PX }}
      >
        {line.text}
      </span>
    </div>
  );
}

function Gap({
  row,
  onExpand,
}: {
  row: GapRow;
  onExpand: (gap: GapRow, mode: "up" | "down" | "all") => void;
}) {
  return (
    <div
      className="bg-muted/30 text-muted-foreground flex items-center justify-center gap-2 font-mono text-[11px]"
      style={{ height: ROW_HEIGHT }}
    >
      <GapButton onClick={() => onExpand(row, "up")} title="Expand 20 above">
        <ChevronUp className="size-3" />
      </GapButton>
      <button
        type="button"
        onClick={() => onExpand(row, "all")}
        className="hover:text-foreground flex items-center gap-1 underline-offset-2 hover:underline"
      >
        <ChevronsUpDown className="size-3" />
        {row.len.toLocaleString()} unchanged
      </button>
      <GapButton onClick={() => onExpand(row, "down")} title="Expand 20 below">
        <ChevronDown className="size-3" />
      </GapButton>
    </div>
  );
}

function GapButton({
  onClick,
  title,
  children,
}: {
  onClick: () => void;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className="hover:text-foreground hover:bg-muted rounded px-1"
    >
      {children}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Chrome
// ---------------------------------------------------------------------------

function Toolbar({
  query,
  setQuery,
  changesOnly,
  setChangesOnly,
  counts,
  onJump,
}: {
  query: string;
  setQuery: (q: string) => void;
  changesOnly: boolean;
  setChangesOnly: (v: boolean) => void;
  counts: { added: number; removed: number; changed: number };
  onJump: (direction: 1 | -1) => void;
}) {
  return (
    <div className="flex shrink-0 items-center gap-3">
      <Input
        placeholder="Search keys and values…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="h-7 max-w-xs text-xs"
      />
      <div className="flex items-center gap-2 font-mono text-[11px]">
        <span className="text-green-600 dark:text-green-400">
          +{counts.added}
        </span>
        <span className="text-red-600 dark:text-red-400">
          -{counts.removed}
        </span>
        <span className="text-amber-600 dark:text-amber-400">
          ~{counts.changed}
        </span>
      </div>
      <div className="flex items-center gap-1">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onJump(-1)}
          title="Previous change"
        >
          <ChevronUp className="size-3" />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onJump(1)}
          title="Next change"
        >
          <ChevronDown className="size-3" />
        </Button>
      </div>
      <label className="flex items-center gap-2 text-xs">
        <Switch checked={changesOnly} onCheckedChange={setChangesOnly} />
        Changes only
      </label>
    </div>
  );
}

function PaneHeader({
  leftLabel,
  rightLabel,
}: {
  leftLabel: string;
  rightLabel: string;
}) {
  return (
    <div className="text-muted-foreground grid shrink-0 grid-cols-2 text-[11px] font-semibold">
      <div className="border-border/50 border-r px-2">{leftLabel}</div>
      <div className="px-2">{rightLabel}</div>
    </div>
  );
}

export type { LineTone };
