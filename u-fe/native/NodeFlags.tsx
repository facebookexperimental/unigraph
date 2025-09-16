// Copyright (c) Meta Platforms, Inc. and affiliates.

/// TypeScript-side utilities mirroring Rust's NodeFlags bitflags
/// See: u-be/unigraph_core/src/types/array_graph.rs (NodeFlags)

// Keep the same primitive representation as Rust's u32 bitflags
export type NodeFlags = number;

// Reuse tier flags layout from Rust (tiers.rs)
export const TIER_FLAGS = [
  0b0001_0000, // Tier idx 0
  0b0010_0000, // Tier idx 1
  0b0100_0000, // Tier idx 2
  0b1000_0000, // Tier idx 3
] as const;

export type TierIdx = 0 | 1 | 2 | 3;

/** Bit masks equivalent to Rust's NodeFlags */
export const NODE_FLAGS = {
  UNREACHABLE: 0b0000_0001,

  TIER_IDX_0: TIER_FLAGS[0],
  TIER_IDX_1: TIER_FLAGS[1],
  TIER_IDX_2: TIER_FLAGS[2],
  TIER_IDX_3: TIER_FLAGS[3],
  ALL_TIERS: 0b1111_0000,
} as const;

/** Returns true if the UNREACHABLE flag is set */
export function isNodeUnreachable(flags: NodeFlags): boolean {
  return (flags & NODE_FLAGS.UNREACHABLE) !== 0;
}

/** Returns the single tier index if exactly one tier bit is set, else null */
export function tierIdx(flags: NodeFlags): TierIdx | null {
  const tierBits = flags & NODE_FLAGS.ALL_TIERS;
  switch (tierBits) {
    case NODE_FLAGS.TIER_IDX_0:
      return 0;
    case NODE_FLAGS.TIER_IDX_1:
      return 1;
    case NODE_FLAGS.TIER_IDX_2:
      return 2;
    case NODE_FLAGS.TIER_IDX_3:
      return 3;
    default:
      return null;
  }
}
