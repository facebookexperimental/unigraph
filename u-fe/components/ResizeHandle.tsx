// Copyright (c) Meta Platforms, Inc. and affiliates.
import { useCallback, useRef, useState } from "react";
import { cn } from "../lib/utils";

// Drag-to-resize for side panels. `useResizableWidth` owns the width (in px),
// persisted to localStorage per `storageKey` so it survives panel switches and
// reloads; `ResizeHandle` is the thin grabbable strip you drop on the panel's
// resizing edge. Pointer capture keeps the drag glued to the handle even when
// the cursor moves fast over sibling elements (e.g. the wgpu canvas).

const MIN_WIDTH = 240;
// Leave room for the icon rail + the rest of the app so the panel can't be
// dragged to fill the whole window.
const VIEWPORT_MARGIN = 200;

export type ResizeHandleProps = {
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (e: React.PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (e: React.PointerEvent<HTMLDivElement>) => void;
};

export function useResizableWidth(
  storageKey: string,
  defaultWidth: number,
): { width: number; handleProps: ResizeHandleProps } {
  const lsKey = `unigraph:panel-width:${storageKey}`;
  const [width, setWidth] = useState(() =>
    readStoredWidth(lsKey, defaultWidth),
  );
  const widthRef = useRef(width);
  const dragRef = useRef<{ startX: number; startWidth: number } | null>(null);

  const update = useCallback((next: number) => {
    widthRef.current = next;
    setWidth(next);
  }, []);

  const onPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    dragRef.current = { startX: e.clientX, startWidth: widthRef.current };
    e.currentTarget.setPointerCapture(e.pointerId);
  }, []);

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      const drag = dragRef.current;
      if (drag == null) {
        return;
      }
      update(clampWidth(drag.startWidth + (e.clientX - drag.startX)));
    },
    [update],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (dragRef.current == null) {
        return;
      }
      dragRef.current = null;
      e.currentTarget.releasePointerCapture(e.pointerId);
      try {
        localStorage.setItem(lsKey, String(widthRef.current));
      } catch {
        // ignore write failures (private mode / quota) — width still applies
        // for this session.
      }
    },
    [lsKey],
  );

  return { width, handleProps: { onPointerDown, onPointerMove, onPointerUp } };
}

/// A thin, grabbable strip pinned to the resizing edge. The parent must be
/// `relative`. Spread `handleProps` from `useResizableWidth` onto it.
export function ResizeHandle({
  className,
  ...handleProps
}: ResizeHandleProps & { className?: string }) {
  return (
    <div
      {...handleProps}
      role="separator"
      aria-orientation="vertical"
      className={cn(
        "absolute top-0 right-0 z-10 h-full w-1.5 cursor-col-resize touch-none",
        "bg-transparent transition-colors hover:bg-primary/40 active:bg-primary/60",
        className,
      )}
    />
  );
}

function clampWidth(px: number): number {
  const max =
    typeof window === "undefined"
      ? Number.POSITIVE_INFINITY
      : Math.max(MIN_WIDTH, window.innerWidth - VIEWPORT_MARGIN);
  return Math.min(Math.max(px, MIN_WIDTH), max);
}

function readStoredWidth(lsKey: string, defaultWidth: number): number {
  if (typeof window === "undefined") {
    return defaultWidth;
  }
  const stored = Number(localStorage.getItem(lsKey));
  return Number.isFinite(stored) && stored > 0
    ? clampWidth(stored)
    : defaultWidth;
}
