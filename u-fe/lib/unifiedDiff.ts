// Copyright (c) Meta Platforms, Inc. and affiliates.

/// A line-oriented unified diff, sized for comparing one node's JSON across
/// the two sides of a delta graph.
///
/// ## Why not just print both sides
///
/// A `GraphNode` is mostly edges, and between two versions of a graph almost
/// all of them are identical. Rendering two panes leaves the reader doing the
/// comparison by eye over hundreds of lines to find the two that moved. A
/// unified diff puts the answer on screen.
///
/// ## Shape
///
/// ```text
///   left ──┐
///          ├─ trimCommon ─→ head + [midL | midR] + tail
///   right ─┘                          │
///                                     ├─ diffMiddle (LCS)
///                                     ↓
///                            collapseContext ─→ DiffLine[]
/// ```
///
/// Trimming the shared head and tail first is what keeps this cheap: two
/// versions of the same node usually differ in one stretch, so the quadratic
/// step runs over a handful of lines rather than the whole document. The LCS
/// table is still bounded — past [`MAX_LCS_CELLS`] the diff declines rather
/// than allocating, and the caller falls back to showing both sides raw.

export type DiffLine =
  | { kind: "context" | "added" | "removed"; text: string }
  | { kind: "gap"; count: number };

export type UnifiedDiff =
  | { t: "identical" }
  | { t: "diff"; lines: DiffLine[] }
  | { t: "too_large"; leftLines: number; rightLines: number };

/// Unchanged lines kept either side of a change, as context.
const CONTEXT_LINES = 3;

/// The LCS table is an `Int32Array` of `n * m`, so this caps it at 4 MB. A
/// node whose changed region is bigger than this is not something anyone is
/// reading line by line anyway.
const MAX_LCS_CELLS = 1_000_000;

// ── Public API ──────────────────────────────────────────────────

export function unifiedJsonDiff(left: unknown, right: unknown): UnifiedDiff {
  return unifiedDiff(stableStringify(left), stableStringify(right));
}

/// Pretty-printed JSON with object keys sorted.
///
/// Key order is an artifact of how each side was deserialized, not something
/// that changed about the node, so leaving it alone would show phantom diffs.
/// Array order is left as-is — for an edge list that IS the data.
export function stableStringify(value: unknown): string {
  return JSON.stringify(sortKeys(value), null, 2);
}

export function unifiedDiff(left: string, right: string): UnifiedDiff {
  if (left === right) {
    return { t: "identical" };
  }

  const leftLines = left.split("\n");
  const rightLines = right.split("\n");
  const { head, tail, midLeft, midRight } = trimCommon(leftLines, rightLines);

  if (midLeft.length * midRight.length > MAX_LCS_CELLS) {
    return {
      t: "too_large",
      leftLines: leftLines.length,
      rightLines: rightLines.length,
    };
  }

  const lines = [
    ...head.map(context),
    ...diffMiddle(midLeft, midRight),
    ...tail.map(context),
  ];

  return { t: "diff", lines: collapseContext(lines) };
}

/// `+`/`-`/` ` prefixes, for copying a diff out as text.
export function renderDiffText(lines: readonly DiffLine[]): string {
  return lines
    .map((line) => {
      switch (line.kind) {
        case "added":
          return `+${line.text}`;
        case "removed":
          return `-${line.text}`;
        case "context":
          return ` ${line.text}`;
        case "gap":
          return `@@ ${line.count} unchanged line${line.count === 1 ? "" : "s"} @@`;
      }
    })
    .join("\n");
}

// ── Implementation ──────────────────────────────────────────────

function context(text: string): DiffLine {
  return { kind: "context", text };
}

function added(text: string): DiffLine {
  return { kind: "added", text };
}

function removed(text: string): DiffLine {
  return { kind: "removed", text };
}

/// Peel off the identical prefix and suffix so the quadratic step only sees
/// the part that actually moved.
function trimCommon(
  left: readonly string[],
  right: readonly string[],
): {
  head: string[];
  tail: string[];
  midLeft: string[];
  midRight: string[];
} {
  const shorter = Math.min(left.length, right.length);

  let start = 0;
  while (start < shorter && left[start] === right[start]) {
    start++;
  }

  let end = 0;
  while (
    end < shorter - start &&
    left[left.length - 1 - end] === right[right.length - 1 - end]
  ) {
    end++;
  }

  return {
    head: left.slice(0, start),
    tail: end === 0 ? [] : left.slice(left.length - end),
    midLeft: left.slice(start, left.length - end),
    midRight: right.slice(start, right.length - end),
  };
}

/// Longest common subsequence, then walk it forwards emitting lines. Ties
/// favour `removed` before `added` so a replaced line reads `-` then `+`.
function diffMiddle(a: readonly string[], b: readonly string[]): DiffLine[] {
  const n = a.length;
  const m = b.length;

  if (n === 0) {
    return b.map(added);
  }
  if (m === 0) {
    return a.map(removed);
  }

  const width = m + 1;
  const lcs = new Int32Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i * width + j] =
        a[i] === b[j]
          ? (lcs[(i + 1) * width + (j + 1)] as number) + 1
          : Math.max(
              lcs[(i + 1) * width + j] as number,
              lcs[i * width + (j + 1)] as number,
            );
    }
  }

  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push(context(a[i] as string));
      i++;
      j++;
    } else if (
      (lcs[(i + 1) * width + j] as number) >=
      (lcs[i * width + (j + 1)] as number)
    ) {
      out.push(removed(a[i] as string));
      i++;
    } else {
      out.push(added(b[j] as string));
      j++;
    }
  }
  while (i < n) {
    out.push(removed(a[i++] as string));
  }
  while (j < m) {
    out.push(added(b[j++] as string));
  }

  return out;
}

/// Replace long runs of unchanged lines with a single gap marker, keeping
/// [`CONTEXT_LINES`] either side of every change.
function collapseContext(lines: readonly DiffLine[]): DiffLine[] {
  const keep = new Array<boolean>(lines.length).fill(false);

  lines.forEach((line, idx) => {
    if (line.kind === "context") {
      return;
    }
    const from = Math.max(0, idx - CONTEXT_LINES);
    const to = Math.min(lines.length - 1, idx + CONTEXT_LINES);
    for (let k = from; k <= to; k++) {
      keep[k] = true;
    }
  });

  const out: DiffLine[] = [];
  let gap = 0;
  for (let i = 0; i < lines.length; i++) {
    if (keep[i] === true) {
      if (gap > 0) {
        out.push({ kind: "gap", count: gap });
        gap = 0;
      }
      out.push(lines[i] as DiffLine);
    } else {
      gap++;
    }
  }
  if (gap > 0) {
    out.push({ kind: "gap", count: gap });
  }

  return out;
}

function sortKeys(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sortKeys);
  }
  if (value != null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(record)
        .sort()
        .map((key) => [key, sortKeys(record[key])]),
    );
  }
  return value;
}
