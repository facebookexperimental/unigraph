// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! `OptionDelta<V, D>` — delta for `Option<T>` fields.

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de::DeserializeOwned;

use crate::Deltable;

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

impl<V> OptionDelta<V, V::Delta>
where
    V: Deltable + Clone,
{
    /// Merge two sequential option deltas: `self` (base→mid) then `other` (mid→target).
    ///
    /// State transition table:
    /// - Unchanged + X → X
    /// - X + Unchanged → X
    /// - _ + Cleared → Cleared
    /// - Cleared + Set(v) → Set(v) (was None, now Some)
    /// - Set(v) + Changed(d) → Set(apply(v, d))
    /// - Set(_) + Cleared → Unchanged (set then cleared = net no-op)
    ///   Actually: from base perspective, if base was None, Set then Cleared = back to None.
    ///   But if base was Some, we can't get Set. So Set only comes from None→Some.
    ///   Set then Cleared means None→Some→None = net unchanged. Correct.
    /// - Changed(d1) + Changed(d2) → Changed(merge(d1, d2))
    /// - Changed(_) + Cleared → Cleared (was Some, changed, then cleared)
    pub fn merge(self, other: OptionDelta<V, V::Delta>) -> OptionDelta<V, V::Delta> {
        match (self, other) {
            // Unchanged is identity
            (OptionDelta::Unchanged, other) => other,
            (first, OptionDelta::Unchanged) => first,

            // Anything then Cleared
            (OptionDelta::Set(_), OptionDelta::Cleared) => {
                // None→Some→None = net unchanged from base perspective
                OptionDelta::Unchanged
            }
            (_, OptionDelta::Cleared) => OptionDelta::Cleared,

            // Cleared then Set (None→None→Some = Set from base)
            (OptionDelta::Cleared, OptionDelta::Set(v)) => OptionDelta::Set(v),

            // Set then Changed: apply the change to the set value
            (OptionDelta::Set(mut v), OptionDelta::Changed(d)) => {
                let _ = v.apply_delta(d);
                OptionDelta::Set(v)
            }

            // Changed then Changed: recursively merge
            (OptionDelta::Changed(d1), OptionDelta::Changed(d2)) => {
                OptionDelta::Changed(V::merge_delta(d1, d2))
            }

            // Changed then Set: the final value is known, just Set it
            // (Some(a)→Some(b)→Some(c) where c is fully known = Set(c) from base)
            // Wait — Changed means base was Some. So the net effect is Changed.
            // But we don't have the original base value to derive a delta to c.
            // Since Set(c) gives us the full value, we can use Set(c) — it's valid
            // for apply_delta to handle Set regardless of current state.
            // Actually no — OptionDelta::Set means "was None, now Some". If we use
            // Set when base was Some, apply_delta would set to Some(c) which is correct.
            // But semantically Changed→Set means the final value is fully known.
            // Using Set here is safe because apply_delta handles Set by just assigning.
            (OptionDelta::Changed(_), OptionDelta::Set(v)) => OptionDelta::Set(v),

            // Cleared then Changed: invalid (mid is None, can't apply Changed to None)
            // This shouldn't happen in valid delta chains. Fall back to Cleared.
            (OptionDelta::Cleared, OptionDelta::Changed(_)) => OptionDelta::Cleared,

            // Set then Set: invalid (mid is Some, d2 Set means mid was None — contradiction)
            // Fall back to the second Set.
            (OptionDelta::Set(_), OptionDelta::Set(v)) => OptionDelta::Set(v),
        }
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
