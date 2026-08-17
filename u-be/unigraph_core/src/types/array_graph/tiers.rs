// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

/// Maximum number of tiers a tiered traversal can define.
///
/// Two things must stay in step with this: the bits reserved for tiers in
/// `EdgeFlags`/`NodeFlags` (see [`TIER_FLAGS`]) and the per-tier stack array in
/// `TieredTraversalIter`. Bits 24..32 of the flag words are still free, so this
/// can grow again if 8 is ever not enough.
pub const MAX_TIERS: usize = 8;

/// Flags that represent tiers that can be reused in different contexts
/// like node_flags or edge flags (for tier transitions).
/// These are defined as a constant to make sure it's consistent across multiple
/// definitions.
///
/// The block sits at bits 16..24. Bits 0..8 hold edge/node type and state flags
/// and bits 8..16 hold the encoded message index on `EdgeFlags`, so the tiers
/// start above both rather than sharing a byte with either.
pub const TIER_FLAGS: [u32; MAX_TIERS] = [
    1 << 16, // Tier idx 0
    1 << 17, // Tier idx 1
    1 << 18, // Tier idx 2
    1 << 19, // Tier idx 3
    1 << 20, // Tier idx 4
    1 << 21, // Tier idx 5
    1 << 22, // Tier idx 6
    1 << 23, // Tier idx 7
];

pub const ALL_TIER_FLAGS: u32 = 0x00FF_0000;

pub fn tier_idx_to_flags(idx: usize) -> Result<u32> {
    TIER_FLAGS.get(idx).copied().ok_or_else(|| {
        anyhow::anyhow!(
            "Tier index {} out of bounds for tiers: {:?}",
            idx,
            TIER_FLAGS
        )
    })
}

/// Inverse of [`tier_idx_to_flags`].
///
/// `bits` is expected to already be masked down to [`ALL_TIER_FLAGS`]. Returns
/// `None` when no tier bit is set, and also when more than one is — a node or
/// edge belongs to exactly one tier, so a multi-bit value is not a tier.
pub fn flags_to_tier_idx(bits: u32) -> Option<usize> {
    TIER_FLAGS.iter().position(|&flag| flag == bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_flags_are_distinct_single_bits_within_the_mask() {
        let mut seen = 0u32;
        for (idx, &flag) in TIER_FLAGS.iter().enumerate() {
            assert_eq!(
                flag.count_ones(),
                1,
                "tier {idx} flag must be a single bit, got {flag:#034b}"
            );
            assert_eq!(
                flag & ALL_TIER_FLAGS,
                flag,
                "tier {idx} flag must live inside ALL_TIER_FLAGS"
            );
            assert_eq!(
                seen & flag,
                0,
                "tier {idx} flag collides with an earlier one"
            );
            seen |= flag;
        }
        assert_eq!(
            seen, ALL_TIER_FLAGS,
            "ALL_TIER_FLAGS must be exactly the union of every tier flag"
        );
    }

    #[test]
    fn test_tier_idx_roundtrips_for_every_tier() -> Result<()> {
        for idx in 0..MAX_TIERS {
            let flags = tier_idx_to_flags(idx)?;
            assert_eq!(flags_to_tier_idx(flags), Some(idx));
        }
        Ok(())
    }

    #[test]
    fn test_tier_idx_out_of_bounds_is_an_error_not_a_panic() {
        assert!(tier_idx_to_flags(MAX_TIERS).is_err());
        assert_eq!(flags_to_tier_idx(0), None);
        // Two tier bits at once is not a tier.
        assert_eq!(flags_to_tier_idx(TIER_FLAGS[0] | TIER_FLAGS[1]), None);
    }
}
