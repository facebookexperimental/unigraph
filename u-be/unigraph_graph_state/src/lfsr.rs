// Copyright (c) Meta Platforms, Inc. and affiliates.

/// Very very simple LFSR implementation for generating pseudo-random numbers, because
/// making `rand` crate work for WASM requires a lot of extra work and a headache.
pub struct Lfsr32 {
    state: u32,
}

impl Lfsr32 {
    pub fn new(seed: u32) -> Self {
        assert!(seed != 0, "Seed must be non-zero");
        Lfsr32 { state: seed }
    }

    /// Advance the LFSR and return the next value as f32 in [-1.0, 1.0)
    pub fn next(&mut self) -> f32 {
        // Feedback taps: 32, 22, 2, 1 (zero-based: 31, 21, 1, 0)
        let bit =
            ((self.state >> 31) ^ (self.state >> 21) ^ (self.state >> 1) ^ (self.state & 1)) & 1;
        self.state = (self.state << 1) | bit;
        // Normalize to [0.0, 1.0)
        let normalized = (self.state as f32) / (u32::MAX as f32);
        // Map to [-1.0, 1.0)
        normalized * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;

    #[test]
    fn test_lfsr32() {
        let mut lfsr = Lfsr32::new(84848484);
        snapshot!(
            [
                lfsr.next(),
                lfsr.next(),
                lfsr.next(),
                lfsr.next(),
                lfsr.next(),
            ],
            "
[
    -0.92097867,
    -0.8419574,
    -0.6839148,
    -0.3678295,
    0.264341,
]
"
        );
    }
}
