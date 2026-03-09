// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Snapshot tests for `Option<T>` `Deltable` impl.

use std::collections::BTreeMap;

use k9::snapshot;

use crate::Deltable;

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap()
}

#[test]
fn none_to_none() {
    let base: Option<i32> = None;
    let target: Option<i32> = None;
    assert!(base.derive_delta(&target).is_none());
}

#[test]
fn some_to_same_some() {
    let base = Some(42);
    let target = Some(42);
    assert!(base.derive_delta(&target).is_none());
}

#[test]
fn some_to_different_some() {
    let base = Some(42);
    let target = Some(99);
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(json(&delta), "99");
}

#[test]
fn some_to_none() {
    let base = Some(42);
    let target: Option<i32> = None;
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "cleared": true
}
"#
    );
}

#[test]
fn none_to_some() {
    let base: Option<i32> = None;
    let target = Some(42);
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "set": 42
}
"#
    );
}

#[test]
fn none_to_some_default() {
    // None → Some(0) should still produce a delta (Set with the full value)
    let base: Option<i32> = None;
    let target = Some(0);
    let delta = base.derive_delta(&target).unwrap();
    // empty_delta for i32 is 0, which is the Set value
    snapshot!(
        json(&delta),
        r#"
{
  "set": 0
}
"#
    );
}

#[test]
fn option_string_changed() {
    let base = Some("hello".to_string());
    let target = Some("world".to_string());
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(json(&delta), r#""world""#);
}

#[test]
fn option_string_cleared() {
    let base = Some("hello".to_string());
    let target: Option<String> = None;
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "cleared": true
}
"#
    );
}

// --- Nested: Option<BTreeMap<String, i32>> ---

#[test]
fn option_map_none_to_some() {
    let base: Option<BTreeMap<String, i32>> = None;
    let target = Some(BTreeMap::from([("a".to_string(), 1)]));
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "set": {
    "a": 1
  }
}
"#
    );
}

#[test]
fn option_map_some_to_none() {
    let base = Some(BTreeMap::from([("a".to_string(), 1)]));
    let target: Option<BTreeMap<String, i32>> = None;
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "cleared": true
}
"#
    );
}

#[test]
fn option_map_some_to_some_with_changes() {
    let base = Some(BTreeMap::from([("a".to_string(), 1)]));
    let target = Some(BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)]));
    let delta = base.derive_delta(&target).unwrap();
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
