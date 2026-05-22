// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Snapshot tests for `BTreeMap<K, V>` `Deltable` impl.

use std::collections::BTreeMap;

use k9::snapshot;

use crate::Deltable;

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap()
}

#[test]
fn unchanged() {
    let a = BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)]);
    assert!(a.derive_delta(&a).is_none());
}

#[test]
fn added_removed_changed() {
    let base = BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)]);
    let target = BTreeMap::from([("b".to_string(), 3), ("c".to_string(), 4)]);
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "added": {
    "c": 4
  },
  "removed": [
    "a"
  ],
  "changed": {
    "b": 3
  }
}
"#
    );
}

#[test]
fn empty_to_nonempty() {
    let base: BTreeMap<String, i32> = BTreeMap::new();
    let target = BTreeMap::from([("a".to_string(), 1)]);
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "added": {
    "a": 1
  }
}
"#
    );
}

#[test]
fn nonempty_to_empty() {
    let base = BTreeMap::from([("a".to_string(), 1)]);
    let target: BTreeMap<String, i32> = BTreeMap::new();
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "removed": [
    "a"
  ]
}
"#
    );
}

#[test]
fn only_added() {
    let base = BTreeMap::from([("a".to_string(), 1)]);
    let target = BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)]);
    let delta = base.derive_delta(&target).unwrap();
    // removed and changed should be omitted from JSON
    snapshot!(
        json(&delta),
        r#"
{
  "added": {
    "b": 2
  }
}
"#
    );
}

#[test]
fn only_changed() {
    let base = BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)]);
    let target = BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 99)]);
    let delta = base.derive_delta(&target).unwrap();
    // added and removed should be omitted from JSON
    snapshot!(
        json(&delta),
        r#"
{
  "changed": {
    "b": 99
  }
}
"#
    );
}

#[test]
fn only_removed() {
    let base = BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)]);
    let target = BTreeMap::from([("a".to_string(), 1)]);
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "removed": [
    "b"
  ]
}
"#
    );
}

// ---------------------------------------------------------------------------
// Recursive struct-valued map delta
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
struct MapConfig {
    pub enabled: bool,
    pub count: u32,
    pub name: String,
}

/// When map values are structs that derive Deltable, `changed` entries
/// contain the per-field delta (only changed fields) instead of the
/// full replacement value.
#[test]
fn recursive_struct_value_delta() {
    let base = BTreeMap::from([
        (
            "prod".to_string(),
            MapConfig {
                enabled: true,
                count: 10,
                name: "production".to_string(),
            },
        ),
        (
            "dev".to_string(),
            MapConfig {
                enabled: false,
                count: 5,
                name: "development".to_string(),
            },
        ),
    ]);
    let target = BTreeMap::from([
        (
            "prod".to_string(),
            MapConfig {
                enabled: true,
                count: 20, // changed
                name: "production".to_string(),
            },
        ),
        (
            "dev".to_string(),
            MapConfig {
                enabled: true, // changed
                count: 5,
                name: "development".to_string(),
            },
        ),
    ]);
    let delta = base.derive_delta(&target).unwrap();
    // `changed` contains per-field deltas, not the full MapConfig struct
    snapshot!(
        json(&delta),
        r#"
{
  "changed": {
    "dev": {
      "enabled": true
    },
    "prod": {
      "count": 20
    }
  }
}
"#
    );
}

/// Roundtrip: derive → apply produces original target.
#[test]
fn recursive_struct_value_roundtrip() {
    let base = BTreeMap::from([(
        "a".to_string(),
        MapConfig {
            enabled: true,
            count: 1,
            name: "alpha".to_string(),
        },
    )]);
    let target = BTreeMap::from([
        (
            "a".to_string(),
            MapConfig {
                enabled: false,
                count: 1,
                name: "alpha_v2".to_string(),
            },
        ),
        (
            "b".to_string(),
            MapConfig {
                enabled: true,
                count: 99,
                name: "beta".to_string(),
            },
        ),
    ]);

    let delta = base.derive_delta(&target).unwrap();
    let mut result = base.clone();
    result.apply_delta(delta).unwrap();
    assert_eq!(result, target);
}
