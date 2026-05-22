// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

/// Flags that represent tiers that can be reused in different contexts
/// like node_flags or edge flags (for tier transitions).
/// These are defined as a constant to make sure it's consistent across multiple
/// definitions.
pub const TIER_FLAGS: [u32; 4] = [
    0b0001_0000, // Tier idx 0
    0b0010_0000, // Tier idx 1
    0b0100_0000, // Tier idx 2
    0b1000_0000, // Tier idx 3
];
pub const ALL_TIER_FLAGS: u32 = 0b1111_0000;

pub fn tier_idx_to_flags(idx: usize) -> Result<u32> {
    match idx {
        0 => Ok(TIER_FLAGS[0]),
        1 => Ok(TIER_FLAGS[1]),
        2 => Ok(TIER_FLAGS[2]),
        3 => Ok(TIER_FLAGS[3]),
        _ => anyhow::bail!(
            "Tier index {} out of bounds for tiers: {:?}",
            idx,
            TIER_FLAGS
        ),
    }
}
