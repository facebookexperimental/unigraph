// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Content-addressed config key types.
//!
//! Each config type has its own key struct (`TraversalConfigKey`, `GraphQueryConfigKey`).
//! Keys are `"{prefix}{xxh3_64_hash_hex}"` — the prefix identifies the type, the hash
//! is computed from the stored blob for deduplication.

use std::fmt;
use std::str::FromStr;

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;

/// Common behavior for all config key types.
pub trait ConfigKeyLike: fmt::Display + fmt::Debug + Clone + Sized {
    const PREFIX: &'static str;

    fn from_blob(blob: &[u8]) -> Self;
    fn as_str(&self) -> &str;
    fn from_string(s: String) -> Result<Self>;
}

/// A row from the configs table, generic over the key type.
#[derive(Debug, Clone)]
pub struct ConfigRow<K> {
    pub key: K,
    /// Compressed blob stored inline (JSON + zstd).
    pub blob_inline: Option<Vec<u8>>,
    /// External blob storage key (for large configs).
    pub blob_id: Option<String>,
}

macro_rules! define_config_key {
    (
        $(#[$meta:meta])*
        $name:ident, $prefix:literal
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash,
            Serialize, Deserialize,
            typegen::TypeGen,
            unigraph_delta::Deltable,
        )]
        #[deltable(replace)]
        pub struct $name(pub String);

        impl ConfigKeyLike for $name {
            const PREFIX: &'static str = $prefix;

            fn from_blob(blob: &[u8]) -> Self {
                let hash = xxhash_rust::xxh3::xxh3_64(blob);
                Self(format!("{}{:016x}", $prefix, hash))
            }

            fn as_str(&self) -> &str {
                &self.0
            }

            fn from_string(s: String) -> Result<Self> {
                anyhow::ensure!(
                    s.starts_with($prefix),
                    "expected {} prefix '{}', got '{}'",
                    stringify!($name),
                    $prefix,
                    &s[..s.len().min(4)]
                );
                Ok(Self(s))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(s: &str) -> Result<Self> {
                Self::from_string(s.to_string())
            }
        }
    };
}

define_config_key!(
    /// Content-addressed key for a `TraversalConfig`.
    ///
    /// Format: `"tvc-{xxh3_64_hex}"` (e.g. `"tvc-1a2b3c4d5e6f7890"`)
    TraversalConfigKey, "tvc-"
);

define_config_key!(
    /// Content-addressed key for a `GraphQueryConfig`.
    ///
    /// Format: `"gqc-{xxh3_64_hex}"` (e.g. `"gqc-abcdef0123456789"`)
    GraphQueryConfigKey, "gqc-"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_from_blob_has_correct_prefix_and_length() {
        let blob = b"test data";
        let tvc = TraversalConfigKey::from_blob(blob);
        assert!(tvc.0.starts_with("tvc-"));
        assert_eq!(tvc.0.len(), 4 + 16);

        let gqc = GraphQueryConfigKey::from_blob(blob);
        assert!(gqc.0.starts_with("gqc-"));
        assert_eq!(gqc.0.len(), 4 + 16);
    }

    #[test]
    fn same_blob_same_key() {
        let blob = b"identical content";
        let k1 = TraversalConfigKey::from_blob(blob);
        let k2 = TraversalConfigKey::from_blob(blob);
        assert_eq!(k1, k2);
    }

    #[test]
    fn different_blobs_different_keys() {
        let k1 = TraversalConfigKey::from_blob(b"aaa");
        let k2 = TraversalConfigKey::from_blob(b"bbb");
        assert_ne!(k1, k2);
    }

    #[test]
    fn parse_roundtrip() {
        let key = TraversalConfigKey::from_blob(b"test");
        let parsed: TraversalConfigKey = key.to_string().parse().unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn parse_wrong_prefix_fails() {
        let err = "gqc-wrong".parse::<TraversalConfigKey>().unwrap_err();
        assert!(err.to_string().contains("tvc-"), "{err}");
    }

    #[test]
    fn different_types_from_same_blob_differ() {
        let blob = b"same";
        let tvc = TraversalConfigKey::from_blob(blob);
        let gqc = GraphQueryConfigKey::from_blob(blob);
        assert_ne!(tvc.as_str(), gqc.as_str());
    }
}
