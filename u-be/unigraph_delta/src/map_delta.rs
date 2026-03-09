// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! `MapDelta<K, V, D>` — delta for `BTreeMap<K, V>` values.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Delta for a `BTreeMap<K, V>`: tracks added, removed, and changed entries.
///
/// - `added`: full values for keys present in target but not base
/// - `removed`: keys present in base but not target
/// - `changed`: per-value deltas (`D`) for keys present in both where the value differs
///
/// The type parameter `D` defaults to `V` (whole-value replacement for leaf types).
/// When `V: Deltable`, `D = V::Delta` — enabling recursive per-field diffing of
/// struct values.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(bound(
    serialize = "K: serde::Serialize + Ord, V: serde::Serialize, D: serde::Serialize",
    deserialize = "K: serde::de::DeserializeOwned + Ord, V: serde::de::DeserializeOwned, D: serde::de::DeserializeOwned"
))]
pub struct MapDelta<K: Ord, V, D = V> {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub added: BTreeMap<K, V>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub removed: BTreeSet<K>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub changed: BTreeMap<K, D>,
}

impl<K: Ord, V, D> MapDelta<K, V, D> {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}
