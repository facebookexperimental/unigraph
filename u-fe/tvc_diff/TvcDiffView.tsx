// Copyright (c) Meta Platforms, Inc. and affiliates.

/**
 * Side-by-side traversal-config diff.
 *
 * Both panes live in ONE scroll container and ONE virtualizer — each row
 * renders both cells into a two-column grid. Two independently scrolled panes
 * would need scroll syncing and would drift apart the moment a row height
 * differed; sharing the row box makes alignment structural instead.
 *
 * Row heights are fixed for the same reason: `getTotalSize()` stays exact, so
 * the scrollbar doesn't jump when a gap expands. Values are truncated with CSS
 * rather than sliced, so the full text is still there for copy and `title`.
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
import type { TraversalConfig } from "../__generated__/ts/TraversalConfig";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Switch } from "../components/ui/switch";
import { cn } from "../lib/utils";
import {
  buildTvcDiff,
  expandGap,
  findNextChange,
  searchTvcDiff,
  type DiffRow,
  type EntryRow,
  type GapRow,
  type TvcDiff,
} from "./TvcDiffModel";

const ROW_HEIGHT = 22;

/// Expanding more than this at once is a visible hitch, so make it deliberate.
const EXPAND_ALL_WARN = 5000;

export default function TvcDiffView({
  left,
  right,
}: {
  left: TraversalConfig | null;
  right: TraversalConfig;
}) {
  const diff = useMemo(() => buildTvcDiff(left, right), [left, right]);
  const [rows, setRows] = useState<readonly DiffRow[]>(diff.rows);
  const [query, setQuery] = useState("");
  const [changesOnly, setChangesOnly] = useState(false);
  const deferredQuery = useDeferredValue(query);
  const scrollRef = useRef<HTMLDivElement>(null);
  const cursor = useRef(-1);

  useEffect(() => setRows(diff.rows), [diff]);

  const visible = useVisibleRows(diff, rows, deferredQuery, changesOnly);

  const virtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  const expand = useCallback(
    (gap: GapRow, mode: "up" | "down" | "all") => {
      if (mode === "all" && gap.len > EXPAND_ALL_WARN) {
        const ok = window.confirm(
          `Expand all ${gap.len.toLocaleString()} unchanged entries? This may take a moment.`,
        );
        if (!ok) return;
      }
      setRows((prev) => {
        const at = prev.indexOf(gap);
        if (at < 0) return prev;
        const next = [...prev];
        next.splice(at, 1, ...expandGap(diff, gap, mode));
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
      const next = findNextChange(visible, from, direction);
      if (next == null) return;
      cursor.current = next;
      virtualizer.scrollToIndex(next, { align: "center" });
    },
    [visible, virtualizer],
  );

  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <Toolbar
        query={query}
        setQuery={setQuery}
        changesOnly={changesOnly}
        setChangesOnly={setChangesOnly}
        counts={countChanges(diff.rows)}
        onJump={jump}
      />
      <PaneHeader />
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
                key={rowKey(row)}
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
          No matching entries.
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
  row: DiffRow;
  onExpand: (gap: GapRow, mode: "up" | "down" | "all") => void;
}) {
  switch (row.kind) {
    case "section":
      return (
        <div className="bg-muted/60 flex h-[22px] items-center gap-2 px-2 font-mono text-[11px] font-semibold">
          <span>{row.section}</span>
          <span className="text-green-600 dark:text-green-400">
            +{row.added}
          </span>
          <span className="text-red-600 dark:text-red-400">-{row.removed}</span>
          <span className="text-amber-600 dark:text-amber-400">
            ~{row.changed}
          </span>
        </div>
      );
    case "gap":
      return <Gap row={row} onExpand={onExpand} />;
    case "truncated":
      return (
        <div className="text-muted-foreground bg-muted/30 h-[22px] px-2 text-center font-mono text-[11px] leading-[22px]">
          showing {row.shown.toLocaleString()} of {row.total.toLocaleString()} —
          use search to find the rest
        </div>
      );
    case "entry":
      return <Entry row={row} />;
  }
}

function Entry({ row }: { row: EntryRow }) {
  const { status, label, left, right } = row;
  return (
    <div className="grid h-[22px] grid-cols-2 font-mono text-[11px] leading-[22px]">
      <Cell
        sign={status === "removed" || status === "changed" ? "-" : " "}
        tone={
          status === "removed" || status === "changed" ? "removed" : "context"
        }
        label={label}
        value={left}
      />
      <Cell
        sign={status === "added" || status === "changed" ? "+" : " "}
        tone={status === "added" || status === "changed" ? "added" : "context"}
        label={label}
        value={right}
      />
    </div>
  );
}

function Cell({
  sign,
  tone,
  label,
  value,
}: {
  sign: string;
  tone: "added" | "removed" | "context";
  label: string;
  value: string | null;
}) {
  if (value == null) {
    return <div className="bg-muted/20 border-border/50 h-full border-r" />;
  }
  return (
    <div
      title={`${label}: ${value}`}
      className={cn(
        "border-border/50 h-full overflow-hidden border-r px-2 text-ellipsis whitespace-nowrap",
        tone === "added" && "bg-green-500/10",
        tone === "removed" && "bg-red-500/10",
      )}
    >
      <span
        className={cn(
          "mr-1 inline-block w-2 select-none",
          tone === "added" && "text-green-600 dark:text-green-400",
          tone === "removed" && "text-red-600 dark:text-red-400",
        )}
      >
        {sign}
      </span>
      <span>{label}</span>
      <span className="text-muted-foreground">: {value}</span>
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
    <div className="bg-muted/30 text-muted-foreground flex h-[22px] items-center justify-center gap-2 font-mono text-[11px]">
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

function PaneHeader() {
  return (
    <div className="text-muted-foreground grid shrink-0 grid-cols-2 text-[11px] font-semibold">
      <div className="border-border/50 border-r px-2">Left</div>
      <div className="px-2">Right</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Search reads the full section data, so it must bypass `rows` entirely —
/// otherwise it could only ever match already-expanded entries.
function useVisibleRows(
  diff: TvcDiff,
  rows: readonly DiffRow[],
  query: string,
  changesOnly: boolean,
): readonly DiffRow[] {
  return useMemo(() => {
    if (query.trim() !== "") return searchTvcDiff(diff, query);
    if (!changesOnly) return rows;
    return rows.filter(
      (r) =>
        r.kind !== "gap" && !(r.kind === "entry" && r.status === "context"),
    );
  }, [diff, rows, query, changesOnly]);
}

function countChanges(rows: readonly DiffRow[]) {
  let added = 0;
  let removed = 0;
  let changed = 0;
  for (const row of rows) {
    if (row.kind !== "section") continue;
    added += row.added;
    removed += row.removed;
    changed += row.changed;
  }
  return { added, removed, changed };
}

function rowKey(row: DiffRow): string {
  switch (row.kind) {
    case "section":
      return `s:${row.section}`;
    case "gap":
      return `g:${row.section}:${row.start}:${row.len}`;
    case "truncated":
      return `t:${row.section}`;
    case "entry":
      return `e:${row.section}:${row.key}`;
  }
}
