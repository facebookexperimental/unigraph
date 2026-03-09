// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Snapshot tests for `#[derive(Deltable)]` with test structs.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use k9::snapshot;

use crate::Deltable;

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap()
}

// ---------------------------------------------------------------------------
// Test structs
// ---------------------------------------------------------------------------

/// Flat struct with leaf fields only.
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
    pub ratio: f64,
}

/// Struct with Option fields (the most common pattern in unigraph).
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

/// Nested struct: outer has a field whose type is also Deltable.
#[derive(
    Deltable,
    Default,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
struct Profile {
    pub name: String,
    pub settings: Option<Settings>,
}

// ---------------------------------------------------------------------------
// Config tests
// ---------------------------------------------------------------------------

#[test]
fn config_no_changes() {
    let a = Config::default();
    assert!(a.derive_delta(&a).is_none());
}

#[test]
fn config_one_field_changed() {
    let base = Config::default();
    let target = Config {
        count: 42,
        ..Default::default()
    };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "count": 42
}
"#
    );
}

#[test]
fn config_all_fields_changed() {
    let base = Config::default();
    let target = Config {
        enabled: true,
        count: 10,
        name: "test".to_string(),
        ratio: 3.125,
    };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "enabled": true,
  "count": 10,
  "name": "test",
  "ratio": 3.125
}
"#
    );
}

// ---------------------------------------------------------------------------
// Settings tests
// ---------------------------------------------------------------------------

#[test]
fn settings_change_label_only() {
    let base = Settings::default();
    let target = Settings {
        label: Some("hello".to_string()),
        ..Default::default()
    };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "label": {
    "set": "hello"
  }
}
"#
    );
}

#[test]
fn settings_clear_tags() {
    let base = Settings {
        tags: Some(BTreeSet::from(["a".to_string(), "b".to_string()])),
        ..Default::default()
    };
    let target = Settings::default();
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "tags": {
    "cleared": true
  }
}
"#
    );
}

#[test]
fn settings_metadata_map_changes() {
    let base = Settings {
        metadata: Some(BTreeMap::from([("key1".to_string(), "val1".to_string())])),
        ..Default::default()
    };
    let target = Settings {
        metadata: Some(BTreeMap::from([
            ("key1".to_string(), "val1".to_string()),
            ("key2".to_string(), "val2".to_string()),
        ])),
        ..Default::default()
    };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "metadata": {
    "added": {
      "key2": "val2"
    }
  }
}
"#
    );
}

// ---------------------------------------------------------------------------
// Profile tests (nested)
// ---------------------------------------------------------------------------

#[test]
fn profile_change_nested_label_only() {
    let base = Profile {
        name: "alice".to_string(),
        settings: Some(Settings {
            label: Some("old".to_string()),
            threshold: Some(0.5),
            ..Default::default()
        }),
    };
    let target = Profile {
        name: "alice".to_string(),
        settings: Some(Settings {
            label: Some("new".to_string()),
            threshold: Some(0.5),
            ..Default::default()
        }),
    };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "settings": {
    "label": "new"
  }
}
"#
    );
}

#[test]
fn profile_clear_settings() {
    let base = Profile {
        name: "alice".to_string(),
        settings: Some(Settings {
            label: Some("test".to_string()),
            ..Default::default()
        }),
    };
    let target = Profile {
        name: "alice".to_string(),
        settings: None,
    };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "settings": {
    "cleared": true
  }
}
"#
    );
}

#[test]
fn profile_set_settings_from_none() {
    let base = Profile {
        name: "alice".to_string(),
        settings: None,
    };
    let target = Profile {
        name: "alice".to_string(),
        settings: Some(Settings {
            label: Some("new".to_string()),
            threshold: Some(0.9),
            ..Default::default()
        }),
    };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "settings": {
    "set": {
      "label": "new",
      "threshold": 0.9,
      "tags": null,
      "metadata": null
    }
  }
}
"#
    );
}

// ---------------------------------------------------------------------------
// #[deltable(replace)] tests
// ---------------------------------------------------------------------------

/// Enum with `#[deltable(replace)]`.
#[derive(
    Deltable,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
#[deltable(replace)]
enum Mode {
    Fast,
    Slow,
    Custom(String),
}

/// Named struct with `#[deltable(replace)]`.
#[derive(
    Deltable,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
#[deltable(replace)]
struct Point {
    pub x: f64,
    pub y: f64,
}

/// Tuple struct with `#[deltable(replace)]`.
#[derive(
    Deltable,
    Clone,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize
)]
#[deltable(replace)]
struct Label(String);

#[test]
fn replace_enum_unchanged() {
    assert!(Mode::Fast.derive_delta(&Mode::Fast).is_none());
}

#[test]
fn replace_enum_changed() {
    let delta = Mode::Fast.derive_delta(&Mode::Slow).unwrap();
    snapshot!(json(&delta), r#""Slow""#);
}

#[test]
fn replace_enum_with_data() {
    let delta = Mode::Fast
        .derive_delta(&Mode::Custom("turbo".into()))
        .unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "Custom": "turbo"
}
"#
    );
}

#[test]
fn replace_enum_apply() {
    let mut val = Mode::Fast;
    val.apply_delta(Mode::Slow).unwrap();
    assert_eq!(val, Mode::Slow);
}

#[test]
fn replace_struct_unchanged() {
    let p = Point { x: 1.0, y: 2.0 };
    assert!(p.derive_delta(&p).is_none());
}

#[test]
fn replace_struct_changed() {
    let base = Point { x: 1.0, y: 2.0 };
    let target = Point { x: 3.0, y: 4.0 };
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(
        json(&delta),
        r#"
{
  "x": 3.0,
  "y": 4.0
}
"#
    );
}

#[test]
fn replace_struct_apply() {
    let mut val = Point { x: 1.0, y: 2.0 };
    val.apply_delta(Point { x: 5.0, y: 6.0 }).unwrap();
    assert_eq!(val, Point { x: 5.0, y: 6.0 });
}

#[test]
fn replace_tuple_struct_unchanged() {
    let a = Label("hello".into());
    assert!(a.derive_delta(&a).is_none());
}

#[test]
fn replace_tuple_struct_changed() {
    let base = Label("hello".into());
    let target = Label("world".into());
    let delta = base.derive_delta(&target).unwrap();
    snapshot!(json(&delta), r#""world""#);
}

#[test]
fn replace_tuple_struct_apply() {
    let mut val = Label("hello".into());
    val.apply_delta(Label("world".into())).unwrap();
    assert_eq!(val, Label("world".into()));
}
