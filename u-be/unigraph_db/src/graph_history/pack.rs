// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;

const ENTRY_BYTES: usize = 12;
const METRIC_ID_BYTES: usize = 4;
const VALUE_BYTES: usize = 8;

pub fn encode_values(values: &BTreeMap<u32, f64>) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(values.len() * ENTRY_BYTES);
    for (metric_id, value) in values {
        encoded.extend_from_slice(&metric_id.to_le_bytes());
        encoded.extend_from_slice(&value.to_le_bytes());
    }
    encoded
}

pub fn decode_values(bytes: &[u8]) -> Result<BTreeMap<u32, f64>> {
    anyhow::ensure!(
        bytes.len().is_multiple_of(ENTRY_BYTES),
        "history value blob length {} is not a multiple of {ENTRY_BYTES}",
        bytes.len()
    );

    let mut values = BTreeMap::new();
    for chunk in bytes.chunks_exact(ENTRY_BYTES) {
        let metric_id = u32::from_le_bytes(
            chunk[..METRIC_ID_BYTES]
                .try_into()
                .context("failed to decode metric id")?,
        );
        let value = f64::from_le_bytes(
            chunk[METRIC_ID_BYTES..METRIC_ID_BYTES + VALUE_BYTES]
                .try_into()
                .context("failed to decode metric value")?,
        );
        values.insert(metric_id, value);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let values = BTreeMap::from([(3, -2.5), (7, 10.0), (42, 0.25)]);

        assert_eq!(
            decode_values(&encode_values(&values)).expect("blob decodes"),
            values
        );
    }

    #[test]
    fn decode_rejects_bad_length() {
        assert!(decode_values(&[1, 2, 3]).is_err());
    }
}
