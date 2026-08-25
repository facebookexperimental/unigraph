// Copyright (c) Meta Platforms, Inc. and affiliates.

/// TypeScript-side utilities mirroring Rust's NodeFlags bitflags
/// See: u-be/unigraph_core/src/types/array_graph.rs (NodeFlags)
/// and u-be/unigraph_core/src/types/array_graph/tiers.rs (TIER_FLAGS)
///
/// Hand-maintained, and nothing checks it. Node flags cross the WASM boundary
/// as a raw `Uint32Array` rather than a generated type — deliberately, since
/// they are read once per row per column — so a layout change on the Rust side
/// produces no compile error here, just silently wrong reads. If you move the
/// bits in `tiers.rs`, move them here too.

// Keep the same primitive representation as Rust's u32 bitflags
export type NodeFlags = number;

/// Maximum number of tiers a tiered traversal can define. Mirrors `MAX_TIERS`.
export const MAX_TIERS = 8;

/// Reuse tier flags layout from Rust (tiers.rs).
///
/// The block sits at bits 16..24. Bits 0..8 hold node state flags and bits
/// 8..16 hold the encoded message index on `EdgeFlags`, so the tiers start
/// above both rather than sharing a byte with either.
export const TIER_FLAGS = [
  1 << 16, // Tier idx 0
  1 << 17, // Tier idx 1
  1 << 18, // Tier idx 2
  1 << 19, // Tier idx 3
  1 << 20, // Tier idx 4
  1 << 21, // Tier idx 5
  1 << 22, // Tier idx 6
  1 << 23, // Tier idx 7
] as const;

export type TierIDX = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7;

/** Bit masks equivalent to Rust's NodeFlags */
export const NODE_FLAGS = {
  UNREACHABLE: 0b0000_0001,

  ALL_TIERS: 0x00ff_0000,
} as const;

/** Returns true if the UNREACHABLE flag is set */
export function isNodeUnreachable(flags: NodeFlags): boolean {
  return (flags & NODE_FLAGS.UNREACHABLE) !== 0;
}

/// Returns the single tier index if exactly one tier bit is set, else null.
///
/// Derived from [`TIER_FLAGS`] rather than a hand-written switch so adding a
/// tier is one line. Mirrors `flags_to_tier_idx`: a zero or multi-bit value is
/// not a tier, and `indexOf` reports both as `-1`.
export function tierIdx(flags: NodeFlags): TierIDX | null {
  const tierBits = flags & NODE_FLAGS.ALL_TIERS;
  const idx = TIER_FLAGS.indexOf(tierBits);
  return idx === -1 ? null : (idx as TierIDX);
}
