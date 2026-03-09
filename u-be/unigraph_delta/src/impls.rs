// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Blanket `Deltable` implementations for standard types.
//!
//! Organized declaratively — easy to scan at a glance.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Debug;

use anyhow::Result;
use anyhow::bail;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Deltable;
use crate::MapDelta;
use crate::OptionDelta;
use crate::SetDelta;

// ---------------------------------------------------------------------------
// Leaf types (Delta = Self, whole-value replacement)
// ---------------------------------------------------------------------------

/// Implement `Deltable` for types where the delta is just the new value.
///
/// This macro is for **foreign/primitive types** that you can't annotate with
/// `#[derive(Deltable)]`. For your own types, prefer:
///
/// ```rust,ignore
/// #[derive(Deltable)]
/// #[deltable(replace)]
/// pub enum MyEnum { ... }
/// ```
///
/// The type must implement `PartialEq` and `Clone`.
///
/// ```rust,ignore
/// impl_deltable_leaf!(bool, u32, String);
/// ```
#[macro_export]
macro_rules! impl_deltable_leaf {
    ($($ty:ty),*) => {
        $(
            impl $crate::Deltable for $ty {
                type Delta = $ty;

                fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
                    if self == other { None } else { Some(other.clone()) }
                }

                fn apply_delta(&mut self, delta: Self::Delta) -> ::anyhow::Result<()> {
                    *self = delta;
                    Ok(())
                }
            }
        )*
    };
}

impl_deltable_leaf!(bool, u8, u16, u32, u64, i8, i16, i32, i64, String);

// f32 and f64 need special PartialEq handling (NaN), but in practice
// our values are well-behaved. Use bitwise comparison for correctness.
impl Deltable for f32 {
    type Delta = f32;

    fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
        if self.to_bits() == other.to_bits() {
            None
        } else {
            Some(*other)
        }
    }

    fn apply_delta(&mut self, delta: Self::Delta) -> Result<()> {
        *self = delta;
        Ok(())
    }
}

impl Deltable for f64 {
    type Delta = f64;

    fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
        if self.to_bits() == other.to_bits() {
            None
        } else {
            Some(*other)
        }
    }

    fn apply_delta(&mut self, delta: Self::Delta) -> Result<()> {
        *self = delta;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Vec<T> (leaf — no stable element identity)
// ---------------------------------------------------------------------------

impl<T> Deltable for Vec<T>
where
    T: PartialEq + Clone + Serialize + DeserializeOwned + Debug,
{
    type Delta = Vec<T>;

    fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
        if self == other {
            None
        } else {
            Some(other.clone())
        }
    }

    fn apply_delta(&mut self, delta: Self::Delta) -> Result<()> {
        *self = delta;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Option<T> (Unchanged / Cleared / Set / Changed)
// ---------------------------------------------------------------------------

impl<T> Deltable for Option<T>
where
    T: Deltable + PartialEq + Clone + Serialize + DeserializeOwned + Debug,
{
    type Delta = OptionDelta<T, T::Delta>;

    fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
        match (self, other) {
            (None, None) => None,
            (Some(_), None) => Some(OptionDelta::Cleared),
            (None, Some(t)) => {
                // None → Some: store the full target value.
                Some(OptionDelta::Set(t.clone()))
            }
            (Some(b), Some(t)) => {
                if b == t {
                    None
                } else {
                    // Values differ — compute inner delta. If the inner
                    // derive_delta returns None despite PartialEq saying the
                    // values differ, fall back to full replacement rather than
                    // silently dropping the change.
                    Some(match b.derive_delta(t) {
                        Some(d) => OptionDelta::Changed(d),
                        None => OptionDelta::Set(t.clone()),
                    })
                }
            }
        }
    }

    fn apply_delta(&mut self, delta: Self::Delta) -> Result<()> {
        match delta {
            OptionDelta::Unchanged => {}
            OptionDelta::Cleared => *self = None,
            OptionDelta::Set(v) => *self = Some(v),
            OptionDelta::Changed(inner) => match self.as_mut() {
                Some(val) => val.apply_delta(inner)?,
                None => bail!("OptionDelta::Changed applied to None — expected Some"),
            },
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// BTreeSet<T> (added/removed elements)
// ---------------------------------------------------------------------------

impl<T> Deltable for BTreeSet<T>
where
    T: Ord + Clone + Serialize + DeserializeOwned + Debug,
{
    type Delta = SetDelta<T>;

    fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
        let added: BTreeSet<T> = other.difference(self).cloned().collect();
        let removed: BTreeSet<T> = self.difference(other).cloned().collect();

        if added.is_empty() && removed.is_empty() {
            None
        } else {
            Some(SetDelta { added, removed })
        }
    }

    fn apply_delta(&mut self, delta: Self::Delta) -> Result<()> {
        for item in delta.removed {
            if !self.remove(&item) {
                bail!(
                    "BTreeSet::apply_delta: removed item {:?} not found in set",
                    item
                );
            }
        }
        for item in delta.added {
            if !self.insert(item.clone()) {
                bail!(
                    "BTreeSet::apply_delta: added item {:?} already exists in set",
                    item
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// BTreeMap<K, V> (added/removed/changed keys, recursive per-value delta)
// ---------------------------------------------------------------------------

impl<K, V> Deltable for BTreeMap<K, V>
where
    K: Ord + Clone + Serialize + DeserializeOwned + Debug,
    V: Deltable + PartialEq + Clone + Serialize + DeserializeOwned + Debug,
{
    type Delta = MapDelta<K, V, V::Delta>;

    fn derive_delta(&self, other: &Self) -> Option<Self::Delta> {
        let mut added = BTreeMap::new();
        let mut removed = BTreeSet::new();
        let mut changed = BTreeMap::new();

        for (k, v) in other {
            match self.get(k) {
                None => {
                    added.insert(k.clone(), v.clone());
                }
                Some(base_v) if base_v != v => {
                    // Values differ — compute inner delta. If the inner
                    // derive_delta returns None despite PartialEq saying the
                    // values differ, fall back to remove + re-add rather than
                    // silently dropping the change.
                    match base_v.derive_delta(v) {
                        Some(d) => {
                            changed.insert(k.clone(), d);
                        }
                        None => {
                            removed.insert(k.clone());
                            added.insert(k.clone(), v.clone());
                        }
                    }
                }
                _ => {} // unchanged
            }
        }
        for k in self.keys() {
            if !other.contains_key(k) {
                removed.insert(k.clone());
            }
        }

        if added.is_empty() && removed.is_empty() && changed.is_empty() {
            None
        } else {
            Some(MapDelta {
                added,
                removed,
                changed,
            })
        }
    }

    fn apply_delta(&mut self, delta: Self::Delta) -> Result<()> {
        for k in delta.removed {
            if self.remove(&k).is_none() {
                bail!(
                    "BTreeMap::apply_delta: removed key {:?} not found in map",
                    k
                );
            }
        }
        for (k, v) in delta.added {
            if self.contains_key(&k) {
                bail!(
                    "BTreeMap::apply_delta: added key {:?} already exists in map",
                    k
                );
            }
            self.insert(k, v);
        }
        for (k, d) in delta.changed {
            match self.get_mut(&k) {
                Some(v) => v.apply_delta(d)?,
                None => bail!(
                    "BTreeMap::apply_delta: changed key {:?} not found in map",
                    k
                ),
            }
        }
        Ok(())
    }
}
