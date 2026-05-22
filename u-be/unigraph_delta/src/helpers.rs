// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Helper functions for diffing collections.
//!
//! These are graph-agnostic utilities used by `unigraph_core`'s graph-level
//! delta derivation. They don't implement `Deltable` but provide lower-level
//! diffing for cases that need custom mapping or key iteration.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::OptionDelta;
use crate::SetDelta;

/// Diff two `Option<T>` values by equality (whole-value replacement).
///
/// Unlike the `Deltable` impl for `Option<T>`, this does NOT produce a
/// field-level inner delta — the delta IS the new value. Useful for `Option<T>`
/// fields where `T` has no meaningful sub-structure to diff (or where you want
/// the delta to contain the full value rather than a `T::Delta`).
pub fn diff_option<T: PartialEq + Clone>(base: &Option<T>, target: &Option<T>) -> OptionDelta<T> {
    if base == target {
        OptionDelta::Unchanged
    } else {
        match target {
            None => OptionDelta::Cleared,
            Some(v) => OptionDelta::Set(v.clone()),
        }
    }
}

/// Apply an [`OptionDelta`] (whole-value replacement) to a mutable `Option<T>`.
pub fn apply_option_delta<T: Clone>(current: &mut Option<T>, delta: &OptionDelta<T>) {
    match delta {
        OptionDelta::Unchanged => {}
        OptionDelta::Cleared => *current = None,
        OptionDelta::Set(v) | OptionDelta::Changed(v) => *current = Some(v.clone()),
    }
}

/// Diff two `BTreeSet<T>`, mapping elements through `map_fn` before collecting.
///
/// Used when base/target contain `NodeIDX` but the delta uses `NodeName`.
pub fn diff_btreeset_mapped<T, U, F>(
    base: &BTreeSet<T>,
    target: &BTreeSet<T>,
    map_fn: F,
) -> Option<SetDelta<U>>
where
    T: Ord,
    U: Ord,
    F: Fn(&T) -> U,
{
    let added: BTreeSet<U> = target.difference(base).map(&map_fn).collect();
    let removed: BTreeSet<U> = base.difference(target).map(&map_fn).collect();

    if added.is_empty() && removed.is_empty() {
        None
    } else {
        Some(SetDelta { added, removed })
    }
}

/// Diff two optional `BTreeMap`s by key.
///
/// Iterates over all keys from both sides and calls
/// `diff_fn(base_value, target_value)` for each key. Only keys where `diff_fn`
/// returns `Some` are included in the result.
///
/// Handles the outer `Option` layer: `None` is treated as an empty map.
pub fn diff_optional_btreemaps<K, V, D, F>(
    base: Option<&BTreeMap<K, V>>,
    target: Option<&BTreeMap<K, V>>,
    diff_fn: F,
) -> BTreeMap<K, D>
where
    K: Ord + Clone,
    F: Fn(Option<&V>, Option<&V>) -> Option<D>,
{
    let empty = BTreeMap::new();
    let base = base.unwrap_or(&empty);
    let target = target.unwrap_or(&empty);

    diff_btreemaps(base, target, diff_fn)
}

/// Diff two `BTreeMap`s by key.
///
/// For each key present in either map, calls `diff_fn(base_value, target_value)`.
/// Only keys where `diff_fn` returns `Some` appear in the result.
pub fn diff_btreemaps<K, V, D, F>(
    base: &BTreeMap<K, V>,
    target: &BTreeMap<K, V>,
    diff_fn: F,
) -> BTreeMap<K, D>
where
    K: Ord + Clone,
    F: Fn(Option<&V>, Option<&V>) -> Option<D>,
{
    let mut result = BTreeMap::new();

    let all_keys: BTreeSet<&K> = base.keys().chain(target.keys()).collect();
    for key in all_keys {
        let base_val = base.get(key);
        let target_val = target.get(key);
        if let Some(delta) = diff_fn(base_val, target_val) {
            result.insert(key.clone(), delta);
        }
    }

    result
}
