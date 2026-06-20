// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Tests for `#[derive(Deltable)]` on enums (field-level variant diffing).
//!
//! Covers the four variant shapes (unit, single-field tuple, multi-field tuple,
//! named), same-variant field-level deltas, cross-variant `Replace`, serde
//! roundtrips, and merge correctness.

use k9::snapshot;

use crate::Deltable;

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap()
}

// ---------------------------------------------------------------------------
// Test types
// ---------------------------------------------------------------------------

/// A field-level struct used as an enum variant payload, mirroring the
/// `Inline(TraversalConfig)` shape that motivated enum diffing.
#[derive(
    Deltable,
    Default,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
struct Inner {
    pub a: u32,
    pub b: Option<String>,
}

/// Enum exercising every variant shape.
#[derive(
    Deltable,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
enum Shape {
    /// Unit variant.
    Empty,
    /// Single-field tuple with a leaf payload.
    Tag(String),
    /// Single-field tuple with a field-level (struct) payload.
    Inline(Inner),
    /// Multi-field tuple.
    Pair(u32, String),
    /// Named (struct) variant.
    Named { x: u32, label: Option<String> },
}

// ---------------------------------------------------------------------------
// Roundtrip + merge helpers
// ---------------------------------------------------------------------------

fn roundtrip<T>(base: &T, target: &T)
where
    T: Deltable + Clone + PartialEq + std::fmt::Debug,
    T::Delta: serde::Serialize + serde::de::DeserializeOwned,
{
    match base.derive_delta(target) {
        None => assert_eq!(base, target),
        Some(d) => {
            let j = serde_json::to_string_pretty(&d).unwrap();
            let de: T::Delta = serde_json::from_str(&j).unwrap();
            let mut result = base.clone();
            result.apply_delta(de).unwrap();
            assert_eq!(&result, target);
        }
    }
}

/// `apply(base, merge(d1, d2)) == apply(apply(base, d1), d2) == target`.
fn assert_merge_roundtrip<T>(base: &T, mid: &T, target: &T)
where
    T: Deltable + Clone + PartialEq + std::fmt::Debug,
{
    let d1 = base.derive_delta(mid);
    let d2 = mid.derive_delta(target);

    let mut sequential = base.clone();
    if let Some(d) = d1.clone() {
        sequential.apply_delta(d).unwrap();
    }
    if let Some(d) = d2.clone() {
        sequential.apply_delta(d).unwrap();
    }

    let merged_delta = match (d1, d2) {
        (None, None) => None,
        (Some(d), None) | (None, Some(d)) => Some(d),
        (Some(d1), Some(d2)) => Some(T::merge_delta(d1, d2)),
    };
    let mut merged = base.clone();
    if let Some(d) = merged_delta {
        merged.apply_delta(d).unwrap();
    }

    assert_eq!(sequential, merged, "merge != sequential");
    assert_eq!(sequential, *target, "did not reach target");
}

// ---------------------------------------------------------------------------
// derive_delta: unchanged
// ---------------------------------------------------------------------------

#[test]
fn unchanged_unit() {
    assert!(Shape::Empty.derive_delta(&Shape::Empty).is_none());
}

#[test]
fn unchanged_tuple() {
    let a = Shape::Tag("x".into());
    assert!(a.derive_delta(&a).is_none());
}

#[test]
fn unchanged_inline() {
    let a = Shape::Inline(Inner {
        a: 1,
        b: Some("hi".into()),
    });
    assert!(a.derive_delta(&a).is_none());
}

#[test]
fn unchanged_named() {
    let a = Shape::Named {
        x: 1,
        label: Some("L".into()),
    };
    assert!(a.derive_delta(&a).is_none());
}

// ---------------------------------------------------------------------------
// derive_delta: same-variant field-level deltas (the key win)
// ---------------------------------------------------------------------------

/// The motivating case: a deep single-field change in a struct-valued variant
/// produces a tiny delta, not the whole payload.
#[test]
fn inline_field_change_is_minimal() {
    let base = Shape::Inline(Inner { a: 1, b: None });
    let target = Shape::Inline(Inner { a: 2, b: None });
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "Inline": {
    "a": 2
  }
}
"#
    );
}

#[test]
fn tag_leaf_change() {
    let delta = Shape::Tag("a".into())
        .derive_delta(&Shape::Tag("b".into()))
        .unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "Tag": "b"
}
"#
    );
}

#[test]
fn pair_partial_change_keeps_positional_nulls() {
    // First field unchanged -> null placeholder; second field changed.
    let delta = Shape::Pair(1, "a".into())
        .derive_delta(&Shape::Pair(1, "b".into()))
        .unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "Pair": [
    null,
    "b"
  ]
}
"#
    );
}

#[test]
fn named_partial_change_skips_unchanged() {
    let base = Shape::Named { x: 1, label: None };
    let target = Shape::Named {
        x: 1,
        label: Some("L".into()),
    };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "Named": {
    "label": {
      "set": "L"
    }
  }
}
"#
    );
}

// ---------------------------------------------------------------------------
// derive_delta: cross-variant Replace
// ---------------------------------------------------------------------------

#[test]
fn variant_switch_is_replace() {
    let delta = Shape::Empty.derive_delta(&Shape::Tag("hi".into())).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "Replace": {
    "Tag": "hi"
  }
}
"#
    );
}

#[test]
fn variant_switch_to_struct_variant_is_replace() {
    let delta = Shape::Tag("x".into())
        .derive_delta(&Shape::Inline(Inner { a: 7, b: None }))
        .unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "Replace": {
    "Inline": {
      "a": 7,
      "b": null
    }
  }
}
"#
    );
}

// ---------------------------------------------------------------------------
// Roundtrips
// ---------------------------------------------------------------------------

#[test]
fn roundtrips() {
    // unchanged
    roundtrip(&Shape::Empty, &Shape::Empty);
    // same-variant changes
    roundtrip(&Shape::Tag("a".into()), &Shape::Tag("b".into()));
    roundtrip(
        &Shape::Inline(Inner { a: 1, b: None }),
        &Shape::Inline(Inner {
            a: 2,
            b: Some("x".into()),
        }),
    );
    roundtrip(&Shape::Pair(1, "a".into()), &Shape::Pair(2, "b".into()));
    roundtrip(
        &Shape::Named { x: 1, label: None },
        &Shape::Named {
            x: 5,
            label: Some("L".into()),
        },
    );
    // cross-variant replacements (every direction we care about)
    roundtrip(&Shape::Empty, &Shape::Tag("t".into()));
    roundtrip(
        &Shape::Tag("t".into()),
        &Shape::Inline(Inner { a: 9, b: None }),
    );
    roundtrip(
        &Shape::Inline(Inner { a: 9, b: None }),
        &Shape::Named { x: 3, label: None },
    );
    roundtrip(&Shape::Named { x: 3, label: None }, &Shape::Empty);
}

// ---------------------------------------------------------------------------
// apply_delta: variant mismatch is an error, not a silent no-op
// ---------------------------------------------------------------------------

#[test]
fn apply_mismatched_variant_delta_errors() {
    // Delta describing an in-variant change to `Inline`...
    let delta = Shape::Inline(Inner { a: 1, b: None })
        .derive_delta(&Shape::Inline(Inner { a: 2, b: None }))
        .unwrap();
    // ...applied to a value that is a different variant.
    let mut val = Shape::Tag("x".into());
    assert!(val.apply_delta(delta).is_err());
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

#[test]
fn merge_same_variant_sequential_field_changes() {
    assert_merge_roundtrip(
        &Shape::Inline(Inner { a: 1, b: None }),
        &Shape::Inline(Inner { a: 2, b: None }),
        &Shape::Inline(Inner {
            a: 2,
            b: Some("done".into()),
        }),
    );
}

#[test]
fn merge_replace_then_in_variant_change() {
    // base -> (replace) Inline -> (in-variant) Inline'
    assert_merge_roundtrip(
        &Shape::Empty,
        &Shape::Inline(Inner { a: 1, b: None }),
        &Shape::Inline(Inner { a: 2, b: None }),
    );
}

#[test]
fn merge_in_variant_change_then_replace() {
    // base -> (in-variant) Inline' -> (replace) Tag
    assert_merge_roundtrip(
        &Shape::Inline(Inner { a: 1, b: None }),
        &Shape::Inline(Inner { a: 2, b: None }),
        &Shape::Tag("end".into()),
    );
}

#[test]
fn merge_two_replaces() {
    assert_merge_roundtrip(
        &Shape::Empty,
        &Shape::Tag("mid".into()),
        &Shape::Named { x: 1, label: None },
    );
}
