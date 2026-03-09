// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! `OptionDelta<V, D>` — delta for `Option<T>` fields.

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de::DeserializeOwned;

/// Delta for an `Option<T>` field.
///
/// Four states:
/// - `Unchanged` — field was not modified (skipped during serialization)
/// - `Cleared` — field was set to `None` (serialized as `{"cleared": true}`)
/// - `Set(V)` — field was set from `None` to `Some(v)` (serialized as `{"set": v}`)
/// - `Changed(D)` — field was updated from `Some` to `Some` (serialized as `d`)
///
/// `V` is the full value type, `D` is the inner delta type. When used as a
/// whole-value replacement (via `diff_option`), `D = V` (the default).
#[derive(Debug, Clone)]
pub enum OptionDelta<V, D = V> {
    Unchanged,
    Cleared,
    Set(V),
    Changed(D),
}

impl<V, D> Default for OptionDelta<V, D> {
    fn default() -> Self {
        OptionDelta::Unchanged
    }
}

impl<V: PartialEq, D: PartialEq> PartialEq for OptionDelta<V, D> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (OptionDelta::Unchanged, OptionDelta::Unchanged) => true,
            (OptionDelta::Cleared, OptionDelta::Cleared) => true,
            (OptionDelta::Set(a), OptionDelta::Set(b)) => a == b,
            (OptionDelta::Changed(a), OptionDelta::Changed(b)) => a == b,
            _ => false,
        }
    }
}

impl<V, D> OptionDelta<V, D> {
    /// Returns `true` if this delta represents no change.
    pub fn is_unchanged(&self) -> bool {
        matches!(self, OptionDelta::Unchanged)
    }
}

// Serde format:
// - Unchanged: skipped entirely (via skip_serializing_if on the field side)
// - Cleared: {"cleared": true}
// - Set: {"set": <value>}
// - Changed: <delta> (serialized directly)
impl<V: Serialize, D: Serialize> Serialize for OptionDelta<V, D> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            OptionDelta::Unchanged => {
                unreachable!("Unchanged should be skipped by skip_serializing_if")
            }
            OptionDelta::Cleared => {
                #[derive(Serialize)]
                struct Cleared {
                    cleared: bool,
                }
                Cleared { cleared: true }.serialize(serializer)
            }
            OptionDelta::Set(v) => {
                #[derive(Serialize)]
                struct SetWrapper<'a, T: Serialize> {
                    set: &'a T,
                }
                SetWrapper { set: v }.serialize(serializer)
            }
            OptionDelta::Changed(d) => d.serialize(serializer),
        }
    }
}

impl<'de, V: DeserializeOwned, D: DeserializeOwned> Deserialize<'de> for OptionDelta<V, D> {
    fn deserialize<De: Deserializer<'de>>(deserializer: De) -> Result<Self, De::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;

        if let Some(obj) = value.as_object() {
            if obj.contains_key("cleared") {
                return Ok(OptionDelta::Cleared);
            }
            if let Some(inner) = obj.get("set") {
                let v = V::deserialize(inner.clone()).map_err(serde::de::Error::custom)?;
                return Ok(OptionDelta::Set(v));
            }
        }

        // Everything else is a Changed delta
        let d = D::deserialize(value).map_err(serde::de::Error::custom)?;
        Ok(OptionDelta::Changed(d))
    }
}
