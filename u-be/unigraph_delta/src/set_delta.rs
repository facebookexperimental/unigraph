// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! `SetDelta<T>` — delta for `BTreeSet<T>` values.

use std::collections::BTreeSet;

/// Delta for a `BTreeSet<T>` — elements added and removed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(bound(
    serialize = "T: serde::Serialize + Ord",
    deserialize = "T: serde::de::DeserializeOwned + Ord"
))]
pub struct SetDelta<T: Ord> {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub added: BTreeSet<T>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub removed: BTreeSet<T>,
}

impl<T: Ord> SetDelta<T> {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}
