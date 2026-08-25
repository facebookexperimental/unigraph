// Copyright (c) Meta Platforms, Inc. and affiliates.

import { expect, test } from "vitest";
import type { AscendingTier } from "../../__generated__/ts/AscendingTier";
import type { Decision } from "../../__generated__/ts/Decision";
import type { TraversalConfig } from "../../__generated__/ts/TraversalConfig";
import {
  buildTvcDiff,
  DEFAULT_DIFF_OPTS,
  expandGap,
  searchTvcDiff,
  type DiffRow,
  type GapRow,
} from "../TvcDiffModel";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

const INC: Decision = { include: true };
const EXC: Decision = { include: false };

/// `force_nodes` from a list of names, all with the same decision.
function nodes(names: string[], decision: Decision = INC): TraversalConfig {
  const force_nodes: Record<string, Decision> = {};
  for (const n of names) force_nodes[n] = decision;
  return { force_nodes };
}

function seqNames(count: number, prefix = "n"): string[] {
  // Zero-padded so lexicographic order matches numeric order — keeps the
  // fixtures readable when a test asserts on gap offsets.
  return Array.from(
    { length: count },
    (_, i) => `${prefix}${String(i).padStart(6, "0")}`,
  );
}

// ---------------------------------------------------------------------------
// Core diff behaviour
// ---------------------------------------------------------------------------

test("identical configs produce a single all-encompassing gap and no hunks", () => {
  const tvc = nodes(seqNames(50));
  const { rows } = buildTvcDiff(tvc, tvc);

  expect(lines(rows)).toEqual(["## force_nodes +0 -0 ~0", "⋯ 50 @0"]);
});

test("a key only on the right is an added row", () => {
  const { rows } = buildTvcDiff(nodes([]), nodes(["a"]));
  expect(lines(rows)).toEqual([
    "## force_nodes +1 -0 ~0",
    '+ a | {"include":true}',
  ]);
});

test("a key only on the left is a removed row", () => {
  const { rows } = buildTvcDiff(nodes(["a"]), nodes([]));
  expect(lines(rows)).toEqual([
    "## force_nodes +0 -1 ~0",
    '- a | {"include":true}',
  ]);
});

test("a key whose value differs is a changed row carrying both sides", () => {
  const { rows } = buildTvcDiff(nodes(["a"], INC), nodes(["a"], EXC));
  expect(lines(rows)).toEqual([
    "## force_nodes +0 -0 ~1",
    '~ a | {"include":true} -> {"include":false}',
  ]);
});

test("a section absent on both sides is omitted entirely", () => {
  const { rows } = buildTvcDiff({}, {});
  expect(rows).toEqual([]);
});

// ---------------------------------------------------------------------------
// Collapsing — the property the whole design rests on
// ---------------------------------------------------------------------------

test("an unchanged run longer than 2x context collapses to one gap row", () => {
  const left = nodes(["a", ...seqNames(100), "z"], INC);
  const right: TraversalConfig = {
    force_nodes: { ...left.force_nodes, z: EXC },
  };

  const { rows } = buildTvcDiff(left, right, {
    ...DEFAULT_DIFF_OPTS,
    contextRows: 3,
  });

  // 101 unchanged keys precede the change; 3 survive as leading context.
  expect(lines(rows)).toEqual([
    "## force_nodes +0 -0 ~1",
    "⋯ 98 @0",
    '  n000097 | {"include":true}',
    '  n000098 | {"include":true}',
    '  n000099 | {"include":true}',
    '~ z | {"include":true} -> {"include":false}',
  ]);
});

test("an unchanged run shorter than 2x context stays as context rows", () => {
  // Two changes separated by 4 unchanged keys, context 3 => no gap, since
  // collapsing 4 rows into a gap row would save nothing.
  const keys = ["a", "b1", "b2", "b3", "b4", "c"];
  const left = nodes(keys, INC);
  const right: TraversalConfig = {
    force_nodes: { ...left.force_nodes, a: EXC, c: EXC },
  };

  const { rows } = buildTvcDiff(left, right, {
    ...DEFAULT_DIFF_OPTS,
    contextRows: 3,
  });

  expect(lines(rows)).toEqual([
    "## force_nodes +0 -0 ~2",
    '~ a | {"include":true} -> {"include":false}',
    '  b1 | {"include":true}',
    '  b2 | {"include":true}',
    '  b3 | {"include":true}',
    '  b4 | {"include":true}',
    '~ c | {"include":true} -> {"include":false}',
  ]);
});

test("trailing context is emitted after the last hunk", () => {
  const left = nodes(["a", ...seqNames(10)], INC);
  const right: TraversalConfig = {
    force_nodes: { ...left.force_nodes, a: EXC },
  };

  const { rows } = buildTvcDiff(left, right, {
    ...DEFAULT_DIFF_OPTS,
    contextRows: 2,
  });

  expect(lines(rows)).toEqual([
    "## force_nodes +0 -0 ~1",
    '~ a | {"include":true} -> {"include":false}',
    '  n000000 | {"include":true}',
    '  n000001 | {"include":true}',
    "⋯ 8 @3",
  ]);
});

// ---------------------------------------------------------------------------
// Correctness traps called out in the design
// ---------------------------------------------------------------------------

test("integer-like keys are ordered lexicographically, not by JS insertion order", () => {
  // JS hoists integer-like keys to the front in numeric order, so iterating
  // the parsed object yields 0,1,2,10 — but Rust's BTreeMap ordered them
  // 0,1,10,2. Trusting insertion order here desyncs every gap offset.
  const tvc = nodes(["10", "2", "0", "1"]);
  const { sections } = buildTvcDiff(tvc, tvc);

  expect(sections.get("force_nodes")?.keys).toEqual(["0", "1", "10", "2"]);
});

test("an omitted optional field equals an explicitly undefined one", () => {
  // Rust skips `message_id` when None. A naive JSON.stringify compare would
  // report a phantom change against a JS-built Decision carrying undefined.
  const left: TraversalConfig = { force_nodes: { a: { include: true } } };
  const right: TraversalConfig = {
    force_nodes: { a: { include: true, message_id: undefined } },
  };

  const { rows } = buildTvcDiff(left, right);
  expect(lines(rows)).toEqual(["## force_nodes +0 -0 ~0", "⋯ 1 @0"]);
});

test("object key order within a value does not register as a change", () => {
  const left: TraversalConfig = {
    force_nodes: { a: { include: true, message_id: "m" } },
  };
  const right: TraversalConfig = {
    force_nodes: { a: { message_id: "m", include: true } as Decision },
  };

  const { rows } = buildTvcDiff(left, right);
  expect(lines(rows)).toEqual(["## force_nodes +0 -0 ~0", "⋯ 1 @0"]);
});

test("force_edges flattens two levels into one labelled row", () => {
  const left: TraversalConfig = { force_edges: { from: { to: INC } } };
  const right: TraversalConfig = { force_edges: { from: { to: EXC } } };

  const { rows } = buildTvcDiff(left, right);
  expect(lines(rows)).toEqual([
    "## force_edges +0 -0 ~1",
    '~ from → to | {"include":true} -> {"include":false}',
  ]);
});

test("force_edges keys containing the display arrow stay distinct", () => {
  // The label is cosmetic; uniqueness must not depend on it.
  const tvc: TraversalConfig = {
    force_edges: { "a → b": { c: INC }, a: { "b → c": EXC } },
  };
  const { sections } = buildTvcDiff(tvc, tvc);
  expect(sections.get("force_edges")?.keys.length).toBe(2);
});

test("a null left side reports every entry as added", () => {
  const { rows } = buildTvcDiff(null, nodes(["a", "b"]));
  expect(lines(rows)).toEqual([
    "## force_nodes +2 -0 ~0",
    '+ a | {"include":true}',
    '+ b | {"include":true}',
  ]);
});

function tier(name: string): AscendingTier {
  return {
    name,
    tags_that_transition_to_this_tier: [],
    dynamic_type_keys_that_transition_to_this_tier: [],
  };
}

function tiered(...names: string[]): TraversalConfig {
  return { tiered_traversal: { AscendingTiers: { tiers: names.map(tier) } } };
}

test("tiered_traversal diffs as a single row", () => {
  // Not a map like every other section — one struct, so one row.
  const { rows } = buildTvcDiff(tiered("a"), tiered("a", "b"));

  expect(rows).toHaveLength(2);
  expect(rows[0]).toMatchObject({ section: "tiered_traversal", changed: 1 });
  expect(rows[1]).toMatchObject({
    kind: "entry",
    label: "tiered_traversal",
    status: "changed",
  });
});

// ---------------------------------------------------------------------------
// The case collapsing cannot save: a wholly-added section
// ---------------------------------------------------------------------------

test("a wholly-added section truncates instead of emitting a row per entry", () => {
  const { rows } = buildTvcDiff({}, nodes(seqNames(10_000)), {
    contextRows: 3,
    maxRowsPerSection: 100,
  });

  const entries = rows.filter((r) => r.kind === "entry");
  expect(entries.length).toBe(100);
  expect(rows.at(-1)).toEqual({
    kind: "truncated",
    section: "force_nodes",
    shown: 100,
    total: 10_000,
  });
});

// ---------------------------------------------------------------------------
// Gap expansion
// ---------------------------------------------------------------------------

test("expanding a gap fully replaces it with context rows", () => {
  const tvc = nodes(seqNames(10));
  const diff = buildTvcDiff(tvc, tvc);
  const gap = diff.rows.find((r) => r.kind === "gap") as GapRow;

  const expanded = expandGap(diff, gap, "all");

  expect(expanded.length).toBe(10);
  expect(lines(expanded)[0]).toBe('  n000000 | {"include":true}');
  expect(lines(expanded).at(-1)).toBe('  n000009 | {"include":true}');
});

test("expanding a gap downwards keeps the remainder collapsed", () => {
  const tvc = nodes(seqNames(100));
  const diff = buildTvcDiff(tvc, tvc);
  const gap = diff.rows.find((r) => r.kind === "gap") as GapRow;

  const expanded = expandGap(diff, gap, "down", 20);

  expect(lines(expanded).slice(0, 2)).toEqual([
    '  n000000 | {"include":true}',
    '  n000001 | {"include":true}',
  ]);
  expect(expanded.at(-1)).toEqual({
    kind: "gap",
    section: "force_nodes",
    start: 20,
    len: 80,
  });
});

test("expanding a gap upwards keeps the remainder collapsed above", () => {
  const tvc = nodes(seqNames(100));
  const diff = buildTvcDiff(tvc, tvc);
  const gap = diff.rows.find((r) => r.kind === "gap") as GapRow;

  const expanded = expandGap(diff, gap, "up", 20);

  expect(expanded[0]).toEqual({
    kind: "gap",
    section: "force_nodes",
    start: 0,
    len: 80,
  });
  expect(lines(expanded).at(-1)).toBe('  n000099 | {"include":true}');
});

// ---------------------------------------------------------------------------
// The invariant the whole design exists to satisfy
// ---------------------------------------------------------------------------

test("100k entries with 50 changes stays under 500 rows", () => {
  const names = seqNames(100_000);
  const left = nodes(names, INC);
  const right = nodes(names, INC);
  for (let i = 0; i < 50; i++) {
    const key = names[i * 1000];
    if (key != null) right.force_nodes![key] = EXC;
  }

  const { rows } = buildTvcDiff(left, right);

  expect(rows.length).toBeLessThan(500);
  expect(
    rows.filter((r) => r.kind === "entry" && r.status === "changed").length,
  ).toBe(50);
});

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

test("search finds entries hidden inside collapsed gaps", () => {
  // The needle sits deep in an unchanged run, so it exists in no materialized
  // row. Searching the rows instead of the section data would miss it.
  const tvc = nodes(seqNames(1000));
  const diff = buildTvcDiff(tvc, tvc);

  expect(diff.rows.some((r) => r.kind === "entry")).toBe(false);
  expect(lines(searchTvcDiff(diff, "n000500"))).toEqual([
    "## force_nodes +0 -0 ~0",
    '  n000500 | {"include":true}',
  ]);
});

test("search matches against values as well as keys", () => {
  const diff = buildTvcDiff(nodes(["a"], INC), nodes(["a"], EXC));
  expect(lines(searchTvcDiff(diff, "false")).length).toBe(2);
});

test("search omits sections with no matches", () => {
  // "beta" appears in no other section, and — unlike "t" — not inside the
  // serialized value `{"include":true}` either.
  const left: TraversalConfig = {
    force_nodes: { alpha: INC },
    force_tagged: { beta: INC },
  };
  const diff = buildTvcDiff(left, left);

  expect(lines(searchTvcDiff(diff, "beta"))).toEqual([
    "## force_tagged +0 -0 ~0",
    '  beta | {"include":true}',
  ]);
});

test("search caps results so a one-character query cannot freeze the view", () => {
  const tvc = nodes(seqNames(50_000));
  const diff = buildTvcDiff(tvc, tvc);

  const results = searchTvcDiff(diff, "n0", 200);
  expect(results.filter((r) => r.kind === "entry").length).toBe(200);
});

// ---------------------------------------------------------------------------
// One table, every section — bird's-eye snapshot
// ---------------------------------------------------------------------------

test("all sections render together", () => {
  const left: TraversalConfig = {
    force_nodes: { keep: INC, drop: INC, flip: INC },
    force_edges: { a: { b: INC } },
    force_tagged: { tag_gone: EXC },
    label_predicates: {},
    messages: { m1: "hello %points_to%" },
  };
  const right: TraversalConfig = {
    force_nodes: { keep: INC, flip: EXC, fresh: INC },
    force_edges: { a: { b: INC, c: EXC } },
    force_tagged: {},
    label_predicates: {},
    messages: { m1: "goodbye %points_to%" },
  };

  expect(lines(buildTvcDiff(left, right).rows).join("\n"))
    .toMatchInlineSnapshot(`
    "## force_nodes +1 -1 ~1
    - drop | {"include":true}
    ~ flip | {"include":true} -> {"include":false}
    + fresh | {"include":true}
      keep | {"include":true}
    ## force_edges +1 -0 ~0
      a → b | {"include":true}
    + a → c | {"include":false}
    ## force_tagged +0 -1 ~0
    - tag_gone | {"include":false}
    ## messages +0 -0 ~1
    ~ m1 | "hello %points_to%" -> "goodbye %points_to%""
  `);
});

// ---------------------------------------------------------------------------
// force_dynamic
// ---------------------------------------------------------------------------

/// A `TraversalConfig` whose `rc:gk` holds `count` gatekeeper overrides.
/// `flipped` names the one whose branch filter differs.
function gkConfig(count: number, flipped?: string): TraversalConfig {
  const overrides: Record<string, { branches: { Include: string[] } }> = {};
  for (let i = 0; i < count; i++) {
    const name = `gk_${String(i).padStart(4, "0")}`;
    overrides[name] = {
      branches: { Include: [name === flipped ? "true" : "false"] },
    };
  }
  return {
    force_dynamic: {
      "rc:gk": { default_branches: { Include: ["false"] }, overrides },
      "rc:MetaConfig.number": {},
    },
  };
}

/// The shape that made this section useless: one type key holds a thousand
/// gatekeeper overrides. Keyed by type alone that is a single row whose value
/// is the whole blob, so flipping one gatekeeper renders as two truncated
/// strings differing somewhere off the right edge of the screen.
///
/// Flattened per override it is one `changed` row, named, with the rest
/// collapsed into gaps.
test("force_dynamic: one flipped gatekeeper is one named row", () => {
  const diff = buildTvcDiff(gkConfig(1000), gkConfig(1000, "gk_0500"));

  expect(lines(diff.rows)).toMatchInlineSnapshot(`
    [
      "## force_dynamic +0 -0 ~1",
      "⋯ 499 @0",
      "  rc:gk · gk_0497 | {"branches":{"Include":["false"]}}",
      "  rc:gk · gk_0498 | {"branches":{"Include":["false"]}}",
      "  rc:gk · gk_0499 | {"branches":{"Include":["false"]}}",
      "~ rc:gk · gk_0500 | {"branches":{"Include":["false"]}} -> {"branches":{"Include":["true"]}}",
      "  rc:gk · gk_0501 | {"branches":{"Include":["false"]}}",
      "  rc:gk · gk_0502 | {"branches":{"Include":["false"]}}",
      "  rc:gk · gk_0503 | {"branches":{"Include":["false"]}}",
      "⋯ 496 @506",
    ]
  `);
});

/// The type's own row is emitted even when it configures nothing, so a type
/// that exists stays visible rather than vanishing into its (absent) overrides.
test("force_dynamic: a type with no overrides still gets a row", () => {
  const diff = buildTvcDiff(
    { force_dynamic: { "rc:MetaConfig.number": {} } },
    { force_dynamic: { "rc:MetaConfig.number": {} } },
  );

  expect(lines(diff.rows)).toMatchInlineSnapshot(`
    [
      "## force_dynamic +0 -0 ~0",
      "⋯ 1 @0",
    ]
  `);
});

/// Adding and removing a single gatekeeper are their own rows too, not a
/// wholesale rewrite of the type.
test("force_dynamic: added and removed overrides are separate rows", () => {
  const left: TraversalConfig = {
    force_dynamic: {
      "rc:gk": {
        default_branches: { Include: ["false"] },
        overrides: { only_in_left: { branches: { Include: ["true"] } } },
      },
    },
  };
  const right: TraversalConfig = {
    force_dynamic: {
      "rc:gk": {
        default_branches: { Include: ["false"] },
        overrides: { only_in_right: { branches: { Include: ["true"] } } },
      },
    },
  };

  expect(lines(buildTvcDiff(left, right).rows)).toMatchInlineSnapshot(`
    [
      "## force_dynamic +1 -1 ~0",
      "  rc:gk · (type defaults) | {"default_branches":{"Include":["false"]}}",
      "- rc:gk · only_in_left | {"branches":{"Include":["true"]}}",
      "+ rc:gk · only_in_right | {"branches":{"Include":["true"]}}",
    ]
  `);
});

/// A change to the type's defaults is distinct from a change to any override.
test("force_dynamic: the type's own defaults change on their own row", () => {
  const left: TraversalConfig = {
    force_dynamic: {
      "rc:gk": {
        default_branches: { Include: ["false"] },
        overrides: { steady: { branches: { Include: ["true"] } } },
      },
    },
  };
  const right: TraversalConfig = {
    force_dynamic: {
      "rc:gk": {
        default_branches: { Include: ["true"] },
        overrides: { steady: { branches: { Include: ["true"] } } },
      },
    },
  };

  expect(lines(buildTvcDiff(left, right).rows)).toMatchInlineSnapshot(`
    [
      "## force_dynamic +0 -0 ~1",
      "~ rc:gk · (type defaults) | {"default_branches":{"Include":["false"]}} -> {"default_branches":{"Include":["true"]}}",
      "  rc:gk · steady | {"branches":{"Include":["true"]}}",
    ]
  `);
});

/// Search has to reach a gatekeeper by name, which is the whole reason the
/// name is a key rather than a substring of one giant value.
test("force_dynamic: a gatekeeper is findable by name inside a gap", () => {
  const diff = buildTvcDiff(gkConfig(1000), gkConfig(1000, "gk_0500"));

  const collapsed = lines(diff.rows).join("\n");
  expect(collapsed).not.toContain("gk_0123");

  expect(lines(searchTvcDiff(diff, "gk_0123"))).toMatchInlineSnapshot(`
    [
      "## force_dynamic +0 -0 ~1",
      "  rc:gk · gk_0123 | {"branches":{"Include":["false"]}}",
    ]
  `);
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compact one-line projection of each row so assertions read like a diff.
function lines(rows: readonly DiffRow[]): string[] {
  return rows.map((row) => {
    switch (row.kind) {
      case "section":
        return `## ${row.section} +${row.added} -${row.removed} ~${row.changed}`;
      case "gap":
        return `⋯ ${row.len} @${row.start}`;
      case "truncated":
        return `… ${row.shown}/${row.total}`;
      case "entry":
        switch (row.status) {
          case "added":
            return `+ ${row.label} | ${row.right}`;
          case "removed":
            return `- ${row.label} | ${row.left}`;
          case "changed":
            return `~ ${row.label} | ${row.left} -> ${row.right}`;
          case "context":
            return `  ${row.label} | ${row.left}`;
        }
    }
  });
}
