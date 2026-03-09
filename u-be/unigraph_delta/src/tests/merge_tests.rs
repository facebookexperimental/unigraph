// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Comprehensive tests for delta merging across all Deltable types.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::Deltable;
use crate::MapDelta;
use crate::OptionDelta;
use crate::SetDelta;

// ---------------------------------------------------------------------------
// Helper: verify merge correctness via roundtrip
// ---------------------------------------------------------------------------

/// Assert that merging two deltas produces the same result as applying them
/// sequentially: `apply(base, merge(d1, d2)) == apply(apply(base, d1), d2)`
fn assert_merge_roundtrip<T>(base: &T, mid: &T, target: &T)
where
    T: Deltable + Clone + PartialEq + std::fmt::Debug,
{
    let d1 = base.derive_delta(mid);
    let d2 = mid.derive_delta(target);

    // Sequential apply
    let mut sequential = base.clone();
    if let Some(d) = d1.clone() {
        sequential.apply_delta(d).unwrap();
    }
    if let Some(d) = d2.clone() {
        sequential.apply_delta(d).unwrap();
    }

    // Merge then apply
    let merged_delta = match (d1, d2) {
        (None, None) => None,
        (Some(d), None) | (None, Some(d)) => Some(d),
        (Some(d1), Some(d2)) => Some(T::merge_delta(d1, d2)),
    };
    let mut merged = base.clone();
    if let Some(d) = merged_delta {
        merged.apply_delta(d).unwrap();
    }

    assert_eq!(
        sequential, merged,
        "merge roundtrip failed: sequential != merged"
    );
    assert_eq!(sequential, *target, "sequential apply didn't reach target");
}

// ---------------------------------------------------------------------------
// Test structs
// ---------------------------------------------------------------------------

#[derive(
    Deltable,
    Default,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
struct Config {
    pub enabled: bool,
    pub count: u32,
    pub name: String,
}

#[derive(
    Deltable,
    Default,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
struct Settings {
    pub label: Option<String>,
    pub threshold: Option<f32>,
    pub tags: Option<BTreeSet<String>>,
    pub metadata: Option<BTreeMap<String, String>>,
}

#[derive(
    Deltable,
    Default,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
struct Nested {
    pub name: String,
    pub config: Option<Config>,
    pub items: BTreeMap<String, Settings>,
}

// ===========================================================================
// Leaf type merge tests
// ===========================================================================

#[test]
fn leaf_merge_second_wins() {
    let d1: u32 = 10;
    let d2: u32 = 20;
    assert_eq!(u32::merge_delta(d1, d2), 20);
}

#[test]
fn leaf_merge_string() {
    let d1 = "hello".to_string();
    let d2 = "world".to_string();
    assert_eq!(String::merge_delta(d1, d2), "world");
}

#[test]
fn leaf_merge_f32() {
    assert_eq!(f32::merge_delta(1.0, 2.0), 2.0);
}

#[test]
fn leaf_merge_bool() {
    assert_eq!(bool::merge_delta(true, false), false);
}

#[test]
fn leaf_merge_roundtrip() {
    assert_merge_roundtrip(&10u32, &20u32, &30u32);
    assert_merge_roundtrip(&"a".to_string(), &"b".to_string(), &"c".to_string());
}

// ===========================================================================
// SetDelta merge tests
// ===========================================================================

#[test]
fn set_merge_add_then_remove_cancels() {
    // base={1,2}, mid={1,2,3}, target={1,2}
    // d1: add 3, d2: remove 3 → merged: empty
    assert_merge_roundtrip(
        &BTreeSet::from([1, 2]),
        &BTreeSet::from([1, 2, 3]),
        &BTreeSet::from([1, 2]),
    );
}

#[test]
fn set_merge_remove_then_add_cancels() {
    // base={1,2,3}, mid={1,2}, target={1,2,3}
    // d1: remove 3, d2: add 3 → merged: empty
    assert_merge_roundtrip(
        &BTreeSet::from([1, 2, 3]),
        &BTreeSet::from([1, 2]),
        &BTreeSet::from([1, 2, 3]),
    );
}

#[test]
fn set_merge_disjoint_adds() {
    // base={1}, mid={1,2}, target={1,2,3}
    assert_merge_roundtrip(
        &BTreeSet::from([1]),
        &BTreeSet::from([1, 2]),
        &BTreeSet::from([1, 2, 3]),
    );
}

#[test]
fn set_merge_disjoint_removes() {
    // base={1,2,3}, mid={1,3}, target={1}
    assert_merge_roundtrip(
        &BTreeSet::from([1, 2, 3]),
        &BTreeSet::from([1, 3]),
        &BTreeSet::from([1]),
    );
}

#[test]
fn set_merge_add_and_remove_different() {
    // base={1,2}, mid={2,3}, target={3,4}
    assert_merge_roundtrip(
        &BTreeSet::from([1, 2]),
        &BTreeSet::from([2, 3]),
        &BTreeSet::from([3, 4]),
    );
}

#[test]
fn set_merge_identity() {
    // Merging with empty delta
    let d1 = SetDelta {
        added: BTreeSet::from([3]),
        removed: BTreeSet::from([1]),
    };
    let d2 = SetDelta {
        added: BTreeSet::new(),
        removed: BTreeSet::new(),
    };
    let merged = d1.clone().merge(d2);
    assert_eq!(merged.added, d1.added);
    assert_eq!(merged.removed, d1.removed);
}

#[test]
fn set_merge_empty_with_nonempty() {
    let d1 = SetDelta::<i32> {
        added: BTreeSet::new(),
        removed: BTreeSet::new(),
    };
    let d2 = SetDelta {
        added: BTreeSet::from([5]),
        removed: BTreeSet::from([2]),
    };
    let merged = d1.merge(d2.clone());
    assert_eq!(merged.added, d2.added);
    assert_eq!(merged.removed, d2.removed);
}

// ===========================================================================
// MapDelta merge tests
// ===========================================================================

#[test]
fn map_merge_add_then_remove_cancels() {
    // base={}, mid={a:1}, target={}
    assert_merge_roundtrip(
        &BTreeMap::<String, u32>::new(),
        &BTreeMap::from([("a".to_string(), 1u32)]),
        &BTreeMap::<String, u32>::new(),
    );
}

#[test]
fn map_merge_add_then_change() {
    // base={}, mid={a:1}, target={a:2}
    assert_merge_roundtrip(
        &BTreeMap::<String, u32>::new(),
        &BTreeMap::from([("a".to_string(), 1u32)]),
        &BTreeMap::from([("a".to_string(), 2u32)]),
    );
}

#[test]
fn map_merge_remove_then_add() {
    // base={a:1}, mid={}, target={a:2}
    assert_merge_roundtrip(
        &BTreeMap::from([("a".to_string(), 1u32)]),
        &BTreeMap::<String, u32>::new(),
        &BTreeMap::from([("a".to_string(), 2u32)]),
    );
}

#[test]
fn map_merge_change_then_change() {
    // base={a:1}, mid={a:2}, target={a:3}
    assert_merge_roundtrip(
        &BTreeMap::from([("a".to_string(), 1u32)]),
        &BTreeMap::from([("a".to_string(), 2u32)]),
        &BTreeMap::from([("a".to_string(), 3u32)]),
    );
}

#[test]
fn map_merge_change_then_remove() {
    // base={a:1,b:2}, mid={a:5,b:2}, target={b:2}
    assert_merge_roundtrip(
        &BTreeMap::from([("a".to_string(), 1u32), ("b".to_string(), 2)]),
        &BTreeMap::from([("a".to_string(), 5u32), ("b".to_string(), 2)]),
        &BTreeMap::from([("b".to_string(), 2u32)]),
    );
}

#[test]
fn map_merge_disjoint_operations() {
    // d1 changes "a", d2 changes "b" — both should appear
    assert_merge_roundtrip(
        &BTreeMap::from([("a".to_string(), 1u32), ("b".to_string(), 2)]),
        &BTreeMap::from([("a".to_string(), 10u32), ("b".to_string(), 2)]),
        &BTreeMap::from([("a".to_string(), 10u32), ("b".to_string(), 20)]),
    );
}

#[test]
fn map_merge_complex_scenario() {
    // d1: add c, remove a, change b
    // d2: add d, remove c (cancels d1 add), change b (merge with d1 change)
    assert_merge_roundtrip(
        &BTreeMap::from([("a".to_string(), 1u32), ("b".to_string(), 2)]),
        &BTreeMap::from([("b".to_string(), 20u32), ("c".to_string(), 3)]),
        &BTreeMap::from([("b".to_string(), 200u32), ("d".to_string(), 4)]),
    );
}

#[test]
fn map_merge_with_struct_values() {
    // BTreeMap<String, Config> — inner merge should be field-level
    let base: BTreeMap<String, Config> = BTreeMap::from([(
        "x".into(),
        Config {
            enabled: true,
            count: 1,
            name: "old".into(),
        },
    )]);
    let mid: BTreeMap<String, Config> = BTreeMap::from([(
        "x".into(),
        Config {
            enabled: false,
            count: 1,
            name: "old".into(),
        },
    )]);
    let target: BTreeMap<String, Config> = BTreeMap::from([(
        "x".into(),
        Config {
            enabled: false,
            count: 99,
            name: "old".into(),
        },
    )]);
    assert_merge_roundtrip(&base, &mid, &target);
}

// ===========================================================================
// OptionDelta merge tests
// ===========================================================================

#[test]
fn option_merge_unchanged_then_x() {
    // Unchanged + Set = Set
    let d1: OptionDelta<String, String> = OptionDelta::Unchanged;
    let d2 = OptionDelta::Set("hello".into());
    let merged = d1.merge(d2);
    assert_eq!(merged, OptionDelta::Set("hello".into()));
}

#[test]
fn option_merge_x_then_unchanged() {
    // Set + Unchanged = Set
    let d1: OptionDelta<String, String> = OptionDelta::Set("hello".into());
    let d2 = OptionDelta::Unchanged;
    let merged = d1.merge(d2);
    assert_eq!(merged, OptionDelta::Set("hello".into()));
}

#[test]
fn option_merge_set_then_cleared() {
    // None→Some→None = net unchanged
    let d1: OptionDelta<String, String> = OptionDelta::Set("hello".into());
    let d2 = OptionDelta::Cleared;
    let merged = d1.merge(d2);
    assert_eq!(merged, OptionDelta::Unchanged);
}

#[test]
fn option_merge_cleared_then_set() {
    // Some→None→Some = net Set
    let d1: OptionDelta<String, String> = OptionDelta::Cleared;
    let d2 = OptionDelta::Set("hello".into());
    let merged = d1.merge(d2);
    assert_eq!(merged, OptionDelta::Set("hello".into()));
}

#[test]
fn option_merge_changed_then_cleared() {
    // Some→Some'→None = net Cleared
    let d1: OptionDelta<u32, u32> = OptionDelta::Changed(10);
    let d2 = OptionDelta::Cleared;
    let merged = d1.merge(d2);
    assert_eq!(merged, OptionDelta::Cleared);
}

#[test]
fn option_merge_changed_then_changed() {
    // For leaf types, Changed(d1) + Changed(d2) = Changed(d2) since leaf merge is last-write-wins
    let d1: OptionDelta<u32, u32> = OptionDelta::Changed(10);
    let d2: OptionDelta<u32, u32> = OptionDelta::Changed(20);
    let merged = d1.merge(d2);
    assert_eq!(merged, OptionDelta::Changed(20));
}

#[test]
fn option_merge_set_then_changed() {
    // None→Some(5)→Some(10): Set(5) + Changed(10) = Set(10) for leaf types
    let d1: OptionDelta<u32, u32> = OptionDelta::Set(5);
    let d2: OptionDelta<u32, u32> = OptionDelta::Changed(10);
    let merged = d1.merge(d2);
    // After merging, the value should be Set(10) since apply(5, 10) = 10 for leaf
    assert_eq!(merged, OptionDelta::Set(10));
}

#[test]
fn option_merge_roundtrip_none_some_none() {
    assert_merge_roundtrip(&None::<u32>, &Some(5), &None);
}

#[test]
fn option_merge_roundtrip_some_none_some() {
    assert_merge_roundtrip(&Some(1u32), &None, &Some(2));
}

#[test]
fn option_merge_roundtrip_some_some_some() {
    assert_merge_roundtrip(&Some(1u32), &Some(2), &Some(3));
}

#[test]
fn option_merge_roundtrip_none_some_some() {
    assert_merge_roundtrip(&None::<u32>, &Some(1), &Some(2));
}

#[test]
fn option_merge_roundtrip_some_some_none() {
    assert_merge_roundtrip(&Some(1u32), &Some(2), &None);
}

// ===========================================================================
// Derived struct merge tests
// ===========================================================================

#[test]
fn struct_merge_disjoint_fields() {
    // d1 changes `enabled`, d2 changes `count`
    let base = Config {
        enabled: true,
        count: 1,
        name: "test".into(),
    };
    let mid = Config {
        enabled: false,
        count: 1,
        name: "test".into(),
    };
    let target = Config {
        enabled: false,
        count: 99,
        name: "test".into(),
    };
    assert_merge_roundtrip(&base, &mid, &target);
}

#[test]
fn struct_merge_overlapping_fields() {
    // Both d1 and d2 change `count` — second should win
    let base = Config {
        enabled: true,
        count: 1,
        name: "test".into(),
    };
    let mid = Config {
        enabled: true,
        count: 50,
        name: "test".into(),
    };
    let target = Config {
        enabled: true,
        count: 100,
        name: "test".into(),
    };
    assert_merge_roundtrip(&base, &mid, &target);
}

#[test]
fn struct_merge_all_fields() {
    let base = Config {
        enabled: true,
        count: 1,
        name: "old".into(),
    };
    let mid = Config {
        enabled: false,
        count: 50,
        name: "mid".into(),
    };
    let target = Config {
        enabled: true,
        count: 100,
        name: "new".into(),
    };
    assert_merge_roundtrip(&base, &mid, &target);
}

#[test]
fn struct_merge_with_option_fields() {
    let base = Settings {
        label: Some("old".into()),
        threshold: Some(0.5),
        tags: Some(BTreeSet::from(["a".into(), "b".into()])),
        metadata: None,
    };
    let mid = Settings {
        label: Some("mid".into()),
        threshold: None, // cleared
        tags: Some(BTreeSet::from(["b".into(), "c".into()])),
        metadata: Some(BTreeMap::from([("key".into(), "val".into())])),
    };
    let target = Settings {
        label: Some("new".into()),
        threshold: Some(0.9), // re-set
        tags: Some(BTreeSet::from(["c".into(), "d".into()])),
        metadata: None, // cleared
    };
    assert_merge_roundtrip(&base, &mid, &target);
}

#[test]
fn struct_merge_nested() {
    let base = Nested {
        name: "base".into(),
        config: Some(Config {
            enabled: true,
            count: 1,
            name: "cfg".into(),
        }),
        items: BTreeMap::from([(
            "item1".into(),
            Settings {
                label: Some("old".into()),
                threshold: Some(0.5),
                tags: None,
                metadata: None,
            },
        )]),
    };
    let mid = Nested {
        name: "mid".into(),
        config: Some(Config {
            enabled: false,
            count: 1,
            name: "cfg".into(),
        }),
        items: BTreeMap::from([
            (
                "item1".into(),
                Settings {
                    label: Some("mid".into()),
                    threshold: Some(0.5),
                    tags: None,
                    metadata: None,
                },
            ),
            (
                "item2".into(),
                Settings {
                    label: None,
                    threshold: Some(1.0),
                    tags: None,
                    metadata: None,
                },
            ),
        ]),
    };
    let target = Nested {
        name: "target".into(),
        config: None, // cleared
        items: BTreeMap::from([(
            "item2".into(),
            Settings {
                label: Some("new".into()),
                threshold: Some(1.0),
                tags: Some(BTreeSet::from(["x".into()])),
                metadata: None,
            },
        )]),
    };
    assert_merge_roundtrip(&base, &mid, &target);
}

// ===========================================================================
// Identity property: merging with empty/no-op delta
// ===========================================================================

#[test]
fn merge_identity_set_delta() {
    let d = SetDelta {
        added: BTreeSet::from([1, 2]),
        removed: BTreeSet::from([3]),
    };
    let empty = SetDelta {
        added: BTreeSet::new(),
        removed: BTreeSet::new(),
    };
    let merged = d.clone().merge(empty);
    assert_eq!(merged.added, d.added);
    assert_eq!(merged.removed, d.removed);
}

#[test]
fn merge_identity_map_delta() {
    let d: MapDelta<String, u32, u32> = MapDelta {
        added: BTreeMap::from([("a".into(), 1)]),
        removed: BTreeSet::from(["b".into()]),
        changed: BTreeMap::from([("c".into(), 99)]),
    };
    let empty: MapDelta<String, u32, u32> = MapDelta {
        added: BTreeMap::new(),
        removed: BTreeSet::new(),
        changed: BTreeMap::new(),
    };
    let merged = d.clone().merge(empty);
    assert_eq!(merged.added, d.added);
    assert_eq!(merged.removed, d.removed);
    assert_eq!(merged.changed, d.changed);
}

// ===========================================================================
// Associativity: merge(merge(d1,d2), d3) == merge(d1, merge(d2,d3))
// ===========================================================================

#[test]
fn merge_associativity_sets() {
    let base = BTreeSet::from([1, 2, 3, 4, 5]);
    let s1 = BTreeSet::from([2, 3, 4, 5, 6]); // remove 1, add 6
    let s2 = BTreeSet::from([3, 4, 6, 7]); // remove 2,5, add 7
    let s3 = BTreeSet::from([4, 7, 8]); // remove 3,6, add 8

    let d1 = base.derive_delta(&s1).unwrap();
    let d2 = s1.derive_delta(&s2).unwrap();
    let d3 = s2.derive_delta(&s3).unwrap();

    let left = d1.clone().merge(d2.clone()).merge(d3.clone());
    let right = d1.merge(d2.merge(d3));

    assert_eq!(left.added, right.added);
    assert_eq!(left.removed, right.removed);
}

#[test]
fn merge_associativity_maps() {
    let base: BTreeMap<String, u32> =
        BTreeMap::from([("a".into(), 1), ("b".into(), 2), ("c".into(), 3)]);
    let s1: BTreeMap<String, u32> =
        BTreeMap::from([("a".into(), 10), ("b".into(), 2), ("d".into(), 4)]);
    let s2: BTreeMap<String, u32> =
        BTreeMap::from([("a".into(), 10), ("d".into(), 40), ("e".into(), 5)]);
    let s3: BTreeMap<String, u32> = BTreeMap::from([("a".into(), 100), ("e".into(), 50)]);

    let d1 = base.derive_delta(&s1).unwrap();
    let d2 = s1.derive_delta(&s2).unwrap();
    let d3 = s2.derive_delta(&s3).unwrap();

    let left = d1.clone().merge(d2.clone()).merge(d3.clone());
    let right = d1.merge(d2.merge(d3));

    assert_eq!(left.added, right.added);
    assert_eq!(left.removed, right.removed);
    assert_eq!(left.changed, right.changed);
}

#[test]
fn merge_associativity_struct() {
    let base = Config {
        enabled: true,
        count: 1,
        name: "a".into(),
    };
    let s1 = Config {
        enabled: false,
        count: 2,
        name: "a".into(),
    };
    let s2 = Config {
        enabled: false,
        count: 3,
        name: "b".into(),
    };
    let s3 = Config {
        enabled: true,
        count: 3,
        name: "c".into(),
    };

    // Verify via roundtrip — the merged result should equal the target
    assert_merge_roundtrip(&base, &s1, &s2);
    assert_merge_roundtrip(&base, &s2, &s3);
    assert_merge_roundtrip(&s1, &s2, &s3);

    // Also verify 3-way merge associativity
    let d1 = base.derive_delta(&s1).unwrap();
    let d2 = s1.derive_delta(&s2).unwrap();
    let d3 = s2.derive_delta(&s3).unwrap();

    let mut left_result = base.clone();
    left_result
        .apply_delta(Config::merge_delta(
            Config::merge_delta(d1.clone(), d2.clone()),
            d3.clone(),
        ))
        .unwrap();

    let mut right_result = base.clone();
    right_result
        .apply_delta(Config::merge_delta(d1, Config::merge_delta(d2, d3)))
        .unwrap();

    assert_eq!(left_result, right_result);
    assert_eq!(left_result, s3);
}
