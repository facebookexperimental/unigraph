// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! Roundtrip tests: derive → serialize → deserialize → apply → assert equal.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use k9::assert_equal;

use crate::Deltable;

// ---------------------------------------------------------------------------
// Test structs (same as derive_tests.rs)
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
    pub ratio: f64,
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
struct Profile {
    pub name: String,
    pub settings: Option<Settings>,
}

// ---------------------------------------------------------------------------
// Roundtrip helper
// ---------------------------------------------------------------------------

fn roundtrip<T>(base: &T, target: &T)
where
    T: Deltable + Clone + PartialEq + std::fmt::Debug,
    T::Delta: serde::Serialize + serde::de::DeserializeOwned,
{
    let delta = base.derive_delta(target);
    match delta {
        None => {
            assert_equal!(base, target);
        }
        Some(d) => {
            // Serde roundtrip of the delta itself
            let json = serde_json::to_string_pretty(&d).unwrap();
            let deserialized: T::Delta = serde_json::from_str(&json).unwrap();
            let re_json = serde_json::to_string_pretty(&deserialized).unwrap();
            assert_equal!(&json, &re_json);

            // Apply the deserialized delta and check result equals target
            let mut result = base.clone();
            result.apply_delta(deserialized).unwrap();
            assert_equal!(&result, target);
        }
    }
}

// ---------------------------------------------------------------------------
// Leaf roundtrips
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_bool() {
    roundtrip(&false, &true);
    roundtrip(&true, &true);
}

#[test]
fn roundtrip_u32() {
    roundtrip(&0u32, &42);
    roundtrip(&42u32, &42);
}

#[test]
fn roundtrip_string() {
    roundtrip(&"hello".to_string(), &"world".to_string());
    roundtrip(&"same".to_string(), &"same".to_string());
}

#[test]
fn roundtrip_vec() {
    roundtrip(&vec![1, 2], &vec![2, 3]);
    roundtrip(&vec![1, 2], &vec![1, 2]);
}

// ---------------------------------------------------------------------------
// Option roundtrips
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_option() {
    roundtrip(&Some(42), &Some(99));
    roundtrip(&Some(42), &None);
    roundtrip(&None, &Some(42));
    roundtrip(&None::<i32>, &None);
    roundtrip(&Some(42), &Some(42));
}

#[test]
fn roundtrip_option_none_to_default() {
    roundtrip(&None, &Some(0i32));
}

// ---------------------------------------------------------------------------
// Set roundtrips
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_btreeset() {
    roundtrip(&BTreeSet::from([1, 2, 3]), &BTreeSet::from([2, 3, 4]));
    roundtrip(&BTreeSet::<i32>::new(), &BTreeSet::from([1, 2]));
    roundtrip(&BTreeSet::from([1, 2]), &BTreeSet::<i32>::new());
    roundtrip(&BTreeSet::from([1, 2, 3]), &BTreeSet::from([1, 2, 3]));
}

// ---------------------------------------------------------------------------
// Map roundtrips
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_btreemap() {
    roundtrip(
        &BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)]),
        &BTreeMap::from([("b".to_string(), 3), ("c".to_string(), 4)]),
    );
    roundtrip(
        &BTreeMap::<String, i32>::new(),
        &BTreeMap::from([("a".to_string(), 1)]),
    );
    roundtrip(
        &BTreeMap::from([("a".to_string(), 1)]),
        &BTreeMap::<String, i32>::new(),
    );
}

// ---------------------------------------------------------------------------
// Struct roundtrips
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_config() {
    roundtrip(
        &Config::default(),
        &Config {
            enabled: true,
            count: 10,
            name: "test".to_string(),
            ratio: 3.125,
        },
    );
    roundtrip(&Config::default(), &Config::default());
}

#[test]
fn roundtrip_settings() {
    roundtrip(
        &Settings::default(),
        &Settings {
            label: Some("hello".to_string()),
            threshold: Some(0.5),
            tags: Some(BTreeSet::from(["a".to_string()])),
            metadata: Some(BTreeMap::from([("k".to_string(), "v".to_string())])),
        },
    );

    // Clear all fields
    roundtrip(
        &Settings {
            label: Some("x".to_string()),
            threshold: Some(1.0),
            tags: Some(BTreeSet::from(["a".to_string()])),
            metadata: Some(BTreeMap::from([("k".to_string(), "v".to_string())])),
        },
        &Settings::default(),
    );
}

#[test]
fn roundtrip_profile_nested() {
    roundtrip(
        &Profile {
            name: "alice".to_string(),
            settings: Some(Settings {
                label: Some("old".to_string()),
                threshold: Some(0.5),
                ..Default::default()
            }),
        },
        &Profile {
            name: "alice".to_string(),
            settings: Some(Settings {
                label: Some("new".to_string()),
                threshold: Some(0.5),
                tags: Some(BTreeSet::from(["tag1".to_string()])),
                ..Default::default()
            }),
        },
    );
}

#[test]
fn roundtrip_profile_clear_settings() {
    roundtrip(
        &Profile {
            name: "alice".to_string(),
            settings: Some(Settings {
                label: Some("test".to_string()),
                ..Default::default()
            }),
        },
        &Profile {
            name: "alice".to_string(),
            settings: None,
        },
    );
}

#[test]
fn roundtrip_profile_set_settings_from_none() {
    roundtrip(
        &Profile {
            name: "alice".to_string(),
            settings: None,
        },
        &Profile {
            name: "alice".to_string(),
            settings: Some(Settings {
                label: Some("new".to_string()),
                threshold: Some(0.9),
                ..Default::default()
            }),
        },
    );
}

// ---------------------------------------------------------------------------
// Recursive map roundtrips (struct-valued BTreeMap)
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_map_with_struct_values() {
    roundtrip(
        &BTreeMap::from([
            (
                "a".to_string(),
                Config {
                    enabled: true,
                    count: 1,
                    name: "alpha".to_string(),
                    ratio: 0.5,
                },
            ),
            (
                "b".to_string(),
                Config {
                    enabled: false,
                    count: 2,
                    name: "beta".to_string(),
                    ratio: 1.0,
                },
            ),
        ]),
        &BTreeMap::from([
            (
                "a".to_string(),
                Config {
                    enabled: true,
                    count: 99, // changed
                    name: "alpha".to_string(),
                    ratio: 0.5,
                },
            ),
            // "b" removed
            (
                "c".to_string(), // added
                Config {
                    enabled: true,
                    count: 3,
                    name: "gamma".to_string(),
                    ratio: 2.0,
                },
            ),
        ]),
    );
}

#[test]
fn roundtrip_map_with_struct_values_unchanged() {
    let m = BTreeMap::from([(
        "x".to_string(),
        Config {
            enabled: true,
            count: 42,
            name: "test".to_string(),
            ratio: 1.5,
        },
    )]);
    roundtrip(&m, &m);
}
