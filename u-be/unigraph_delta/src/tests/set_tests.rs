// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Snapshot tests for `BTreeSet<T>` `Deltable` impl.

use std::collections::BTreeSet;

use k9::snapshot;

use crate::Deltable;

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap()
}

#[test]
fn unchanged() {
    let a = BTreeSet::from([1, 2, 3]);
    assert!(a.derive_delta(&a).is_none());
}

#[test]
fn added_and_removed() {
    let base = BTreeSet::from([1, 2, 3]);
    let target = BTreeSet::from([2, 3, 4]);
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "added": [
    4
  ],
  "removed": [
    1
  ]
}
"#
    );
}

#[test]
fn empty_to_nonempty() {
    let base: BTreeSet<i32> = BTreeSet::new();
    let target = BTreeSet::from([1, 2]);
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "added": [
    1,
    2
  ]
}
"#
    );
}

#[test]
fn nonempty_to_empty() {
    let base = BTreeSet::from([1, 2]);
    let target: BTreeSet<i32> = BTreeSet::new();
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "removed": [
    1,
    2
  ]
}
"#
    );
}

#[test]
fn string_sets_ordering() {
    let base = BTreeSet::from(["a".to_string(), "b".to_string()]);
    let target = BTreeSet::from(["b".to_string(), "c".to_string()]);
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "added": [
    "c"
  ],
  "removed": [
    "a"
  ]
}
"#
    );
}
