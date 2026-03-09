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

impl<T: Ord + Clone> SetDelta<T> {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    /// Merge two sequential set deltas: `self` (base→mid) then `other` (mid→target).
    ///
    /// - Items added in d1 then removed in d2 cancel out.
    /// - Items removed in d1 then re-added in d2 cancel out.
    /// - Disjoint additions/removals combine.
    pub fn merge(self, other: SetDelta<T>) -> SetDelta<T> {
        // added = (d1.added \ d2.removed) ∪ (d2.added \ d1.removed)
        let added: BTreeSet<T> = self
            .added
            .iter()
            .filter(|item| !other.removed.contains(item))
            .cloned()
            .chain(
                other
                    .added
                    .iter()
                    .filter(|item| !self.removed.contains(item))
                    .cloned(),
            )
            .collect();

        // removed = (d1.removed \ d2.added) ∪ (d2.removed \ d1.added)
        let removed: BTreeSet<T> = self
            .removed
            .iter()
            .filter(|item| !other.added.contains(item))
            .cloned()
            .chain(
                other
                    .removed
                    .iter()
                    .filter(|item| !self.added.contains(item))
                    .cloned(),
            )
            .collect();

        SetDelta { added, removed }
    }
}
