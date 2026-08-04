// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useVirtualizer } from "@tanstack/react-virtual";
import { useDeferredValue, useMemo, useRef, useState } from "react";
import { Input } from "../components/ui/input";

const ROW_HEIGHT = 32;
const MAX_LIST_HEIGHT = 320;

/// Search + windowed list for traversal config sections.
///
/// Config sections routinely hold hundreds of thousands of entries, so nothing
/// here may be O(entries) in React elements: only the rows actually on screen
/// are rendered, and filtering runs against plain strings rather than nodes.
export default function VirtualEntryList<T>({
  items,
  searchKey,
  renderRow,
  rowKey,
  placeholder,
}: {
  items: readonly T[];
  /// Text a row is matched against. Kept separate from `renderRow` so
  /// filtering never has to build a row to decide whether to show it.
  searchKey: (item: T) => string;
  renderRow: (item: T) => React.ReactNode;
  rowKey: (item: T) => string;
  placeholder?: string;
}) {
  const [query, setQuery] = useState("");
  // Keeps typing responsive on huge lists — the filter lags a frame instead of
  // blocking each keystroke.
  const deferredQuery = useDeferredValue(query);
  const parentRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    const needle = deferredQuery.trim().toLowerCase();
    if (needle === "") return items;
    return items.filter((item) =>
      searchKey(item).toLowerCase().includes(needle),
    );
  }, [items, deferredQuery, searchKey]);

  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  return (
    <div className="flex flex-col gap-1">
      <Input
        placeholder={placeholder ?? "Search…"}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="h-7 text-xs"
      />
      <div className="text-[11px] text-muted-foreground">
        {filtered.length === items.length
          ? `${items.length.toLocaleString()} entries`
          : `${filtered.length.toLocaleString()} of ${items.length.toLocaleString()}`}
      </div>
      <div
        ref={parentRef}
        className="overflow-auto"
        style={{
          height: Math.min(filtered.length * ROW_HEIGHT, MAX_LIST_HEIGHT),
        }}
      >
        <div
          className="relative w-full"
          style={{ height: virtualizer.getTotalSize() }}
        >
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const item = filtered[virtualRow.index];
            if (item == null) return null;
            return (
              <div
                key={rowKey(item)}
                className="absolute top-0 left-0 w-full"
                style={{
                  height: virtualRow.size,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {renderRow(item)}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
