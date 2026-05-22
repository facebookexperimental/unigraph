// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Snapshot tests for leaf type `Deltable` impls.

use k9::snapshot;

use crate::Deltable;

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap()
}

#[test]
fn bool_changed() {
    let delta = false.derive_delta(&true).unwrap();
    snapshot!(json(&delta), "true");
}

#[test]
fn bool_unchanged() {
    assert!(true.derive_delta(&true).is_none());
}

#[test]
fn u32_changed() {
    let delta = 0u32.derive_delta(&42).unwrap();
    snapshot!(json(&delta), "42");
}

#[test]
fn u32_unchanged() {
    assert!(42u32.derive_delta(&42).is_none());
}

#[test]
fn i64_changed() {
    let delta = (-10i64).derive_delta(&20).unwrap();
    snapshot!(json(&delta), "20");
}

#[test]
fn f32_changed() {
    let delta = 1.5f32.derive_delta(&2.5).unwrap();
    snapshot!(json(&delta), "2.5");
}

#[test]
fn f64_changed() {
    let delta = 1.5f64.derive_delta(&2.5).unwrap();
    snapshot!(json(&delta), "2.5");
}

#[test]
fn f64_unchanged() {
    assert!(3.125f64.derive_delta(&3.125).is_none());
}

#[test]
fn string_changed() {
    let delta = "hello"
        .to_string()
        .derive_delta(&"world".to_string())
        .unwrap();
    snapshot!(json(&delta), r#""world""#);
}

#[test]
fn string_unchanged() {
    assert!(
        "same"
            .to_string()
            .derive_delta(&"same".to_string())
            .is_none()
    );
}

#[test]
fn vec_changed() {
    let base = vec![1, 2];
    let target = vec![2, 3];
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        "
[
  2,
  3
]
"
    );
}

#[test]
fn vec_unchanged() {
    let v = vec![1, 2, 3];
    assert!(v.derive_delta(&v).is_none());
}

#[test]
fn vec_empty_to_nonempty() {
    let base: Vec<i32> = vec![];
    let target = vec![1, 2];
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        "
[
  1,
  2
]
"
    );
}
