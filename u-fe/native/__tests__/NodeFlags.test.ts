// Copyright (c) Meta Platforms, Inc. and affiliates.

import { expect, test } from "vitest";
import {
  isNodeUnreachable,
  MAX_TIERS,
  NODE_FLAGS,
  TIER_FLAGS,
  tierIdx,
} from "../NodeFlags";

/// Pinned against `TIER_FLAGS` / `ALL_TIER_FLAGS` in
/// `u-be/unigraph_core/src/types/array_graph/tiers.rs`. These two files are
/// kept in step by hand — node flags cross the WASM boundary as a raw
/// `Uint32Array`, so nothing catches a drift at compile time.
test("tier bit layout matches the Rust side", () => {
  expect(TIER_FLAGS.length).toBe(MAX_TIERS);
  expect(TIER_FLAGS.map((flag) => flag.toString(16))).toMatchInlineSnapshot(`
    [
      "10000",
      "20000",
      "40000",
      "80000",
      "100000",
      "200000",
      "400000",
      "800000",
    ]
  `);
  expect(NODE_FLAGS.ALL_TIERS).toBe(
    TIER_FLAGS.reduce((all, flag) => all | flag, 0),
  );
});

test("every tier index round-trips through its flag", () => {
  const table = TIER_FLAGS.map(
    (flag, idx) =>
      `${idx} → 0x${flag.toString(16).padStart(6, "0")} → ${tierIdx(flag)}`,
  ).join("\n");

  expect(table).toMatchInlineSnapshot(`
    "0 → 0x010000 → 0
    1 → 0x020000 → 1
    2 → 0x040000 → 2
    3 → 0x080000 → 3
    4 → 0x100000 → 4
    5 → 0x200000 → 5
    6 → 0x400000 → 6
    7 → 0x800000 → 7"
  `);
});

/// The bug this file was written after: the tier block moved from bits 4..8 to
/// bits 16..24 on the Rust side and this mirror kept reading the old window,
/// so every node reported "no tier" and the Tier column rendered blank.
test("bits outside the tier block are not read as a tier", () => {
  const cases: Array<[label: string, flags: number]> = [
    ["no flags", 0],
    ["unreachable only", NODE_FLAGS.UNREACHABLE],
    ["old tier window (bits 4..8)", 0b1111_0000],
    ["message index (bits 8..16)", 0x0000_ff00],
    ["two tier bits at once", TIER_FLAGS[0] | TIER_FLAGS[1]],
    ["above the tier block (bit 24)", 1 << 24],
  ];

  for (const [label, flags] of cases) {
    expect(tierIdx(flags), label).toBeNull();
  }

  // A real tier survives the other flags being set alongside it.
  expect(tierIdx(TIER_FLAGS[4] | NODE_FLAGS.UNREACHABLE | 0xff00)).toBe(4);
});

test("isNodeUnreachable reads only the low bit", () => {
  expect(isNodeUnreachable(0)).toBe(false);
  expect(isNodeUnreachable(NODE_FLAGS.UNREACHABLE)).toBe(true);
  expect(isNodeUnreachable(TIER_FLAGS[2])).toBe(false);
  expect(isNodeUnreachable(TIER_FLAGS[2] | NODE_FLAGS.UNREACHABLE)).toBe(true);
});
