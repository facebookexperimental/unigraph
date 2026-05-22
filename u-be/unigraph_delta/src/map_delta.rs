// Copyright (c) Meta Platforms, Inc. and affiliates.

//! `MapDelta<K, V, D>` — delta for `BTreeMap<K, V>` values.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::Deltable;

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

impl<K, V> MapDelta<K, V, V::Delta>
where
    K: Ord + Clone,
    V: Deltable + Clone,
{
    /// Merge two sequential map deltas: `self` (base→mid) then `other` (mid→target).
    ///
    /// Handles all interactions:
    /// - d1.added + d2.removed → cancel out (net no-op)
    /// - d1.added + d2.changed → merged.added with delta applied to value
    /// - d1.removed + d2.added → merged.added (net replacement)
    /// - d1.changed + d2.changed → recursively merged inner deltas
    /// - d1.changed + d2.removed → merged.removed
    /// - Keys only in one delta → pass through
    pub fn merge(self, other: MapDelta<K, V, V::Delta>) -> MapDelta<K, V, V::Delta> {
        let mut added: BTreeMap<K, V> = BTreeMap::new();
        let mut removed: BTreeSet<K> = BTreeSet::new();
        let mut changed: BTreeMap<K, V::Delta> = BTreeMap::new();

        // Track which keys from d2 we've already handled via d1 interactions
        let mut d2_handled_added: BTreeSet<K> = BTreeSet::new();
        let mut d2_handled_removed: BTreeSet<K> = BTreeSet::new();
        let mut d2_handled_changed: BTreeSet<K> = BTreeSet::new();

        // Process d1.added: each key was added in base→mid
        for (k, mut v) in self.added {
            if other.removed.contains(&k) {
                // added then removed → cancel (net no-op from base perspective)
                d2_handled_removed.insert(k);
            } else if let Some(d) = other.changed.get(&k) {
                // added then changed → add with modified value
                let _ = v.apply_delta(d.clone());
                added.insert(k.clone(), v);
                d2_handled_changed.insert(k);
            } else {
                // added, untouched by d2 → stays added
                added.insert(k, v);
            }
        }

        // Process d1.removed: each key was removed in base→mid
        for k in self.removed {
            if let Some(v) = other.added.get(&k) {
                // removed then re-added with new value → net: remove + add (replace)
                // We need both because we don't have the base value to compute
                // a `changed` delta.
                removed.insert(k.clone());
                added.insert(k.clone(), v.clone());
                d2_handled_added.insert(k);
            } else {
                // removed, not re-added → stays removed
                removed.insert(k);
            }
        }

        // Process d1.changed: each key was changed in base→mid
        for (k, d1_inner) in self.changed {
            if other.removed.contains(&k) {
                // changed then removed → net removal
                removed.insert(k.clone());
                d2_handled_removed.insert(k);
            } else if let Some(d2_inner) = other.changed.get(&k) {
                // changed then changed → recursively merge inner deltas
                changed.insert(k.clone(), V::merge_delta(d1_inner, d2_inner.clone()));
                d2_handled_changed.insert(k);
            } else {
                // changed, untouched by d2 → stays changed
                changed.insert(k, d1_inner);
            }
        }

        // Process remaining d2.added (not already handled via d1.removed interaction)
        for (k, v) in other.added {
            if !d2_handled_added.contains(&k) {
                added.insert(k, v);
            }
        }

        // Process remaining d2.removed (not already handled)
        for k in other.removed {
            if !d2_handled_removed.contains(&k) {
                removed.insert(k);
            }
        }

        // Process remaining d2.changed (not already handled)
        for (k, d) in other.changed {
            if !d2_handled_changed.contains(&k) {
                changed.insert(k, d);
            }
        }

        MapDelta {
            added,
            removed,
            changed,
        }
    }
}
