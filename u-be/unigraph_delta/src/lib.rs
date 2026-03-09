// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

//! # `unigraph_delta` — Field-level struct diffing with `#[derive(Deltable)]`
//!
//! This crate provides the [`Deltable`] trait and a derive macro for computing
//! compact, field-level diffs between two instances of a struct and applying
//! those diffs back.
//!
//! ## When to use
//!
//! Use `#[derive(Deltable)]` on any struct where you want to:
//! - Track which fields changed between two versions
//! - Serialize only the changed fields (compact deltas)
//! - Apply those changes back to reconstruct the target state
//!
//! ## The `Deltable` trait
//!
//! ```rust,ignore
//! pub trait Deltable: Sized {
//!     type Delta: Serialize + DeserializeOwned + Clone + Debug;
//!
//!     /// Compute delta from `self` to `other`. Returns `None` if equal.
//!     fn derive_delta(&self, other: &Self) -> Option<Self::Delta>;
//!
//!     /// Apply delta in place — returns `Err` if the delta is invalid.
//!     fn apply_delta(&mut self, delta: Self::Delta) -> anyhow::Result<()>;
//! }
//! ```
//!
//! Key design choices:
//! - **Instance methods**: `base.derive_delta(&target)` reads naturally.
//! - **`apply_delta` takes `delta` by value** (consumes it — moves, no clones).
//!   The delta is small; the data being patched may be massive.
//! - **`apply_delta` returns `Result`** so invalid deltas (e.g. referencing
//!   non-existent map keys) produce clear errors instead of silent data loss.
//!
//! ## How blanket impls work
//!
//! The crate provides `Deltable` implementations for standard types. The proc
//! macro just calls `self.field.derive_delta(&other.field)` for each field —
//! Rust's trait dispatch picks the right impl automatically. No field attributes
//! needed.
//!
//! ### Primitive types (delta = replacement value)
//!
//! `bool`, `u8`..`u64`, `i8`..`i64`, `f32`, `f64`, `String`, `Vec<T>`.
//!
//! For these types, `Delta = Self`. If the values differ, the delta IS the new
//! value. `Vec<T>` is also treated this way because elements have no stable
//! identity.
//!
//! ### `Option<T>` where `T: Deltable`
//!
//! `Delta = OptionDelta<T, T::Delta>` — four states:
//! - `Unchanged` — field was not modified (skipped during serialization)
//! - `Cleared` — field was set to `None` (serialized as `{"cleared": true}`)
//! - `Set(T)` — field was set from `None` to `Some` (serialized as `{"set": v}`)
//! - `Changed(T::Delta)` — field was updated from `Some` to `Some` (serialized
//!   as the delta directly)
//!
//! The `Set` variant carries the full value `T`, so `Option<T>` does NOT require
//! `T: Default`. The `Changed` variant carries a field-level inner delta.
//!
//! ### `BTreeSet<T>` — per-element diff
//!
//! `Delta = SetDelta<T>` — tracks `added` and `removed` elements.
//!
//! ### `BTreeMap<K, V>` — per-key diff with recursive value deltas
//!
//! `Delta = MapDelta<K, V, V::Delta>` — tracks `added`, `removed`, and `changed`
//! entries. For added entries, the full value `V` is stored. For changed entries,
//! only `V::Delta` is stored — enabling recursive per-field diffing when `V` is
//! a struct with `#[derive(Deltable)]`. For replacement types where
//! `V::Delta == V`, this is equivalent to whole-value replacement.
//!
//! **Important**: `apply_delta` on `BTreeMap` will return an error if a `changed`
//! key does not exist in the map. This catches bugs early instead of silently
//! dropping changes.
//!
//! ## `#[derive(Deltable)]` — field-level diffing (default)
//!
//! Given:
//! ```rust,ignore
//! #[derive(Deltable)]
//! struct Config {
//!     pub enabled: bool,
//!     pub name: String,
//! }
//! ```
//!
//! The macro generates `ConfigDelta`:
//! ```rust,ignore
//! struct ConfigDelta {
//!     enabled: Option<bool>,    // None = unchanged, Some(v) = new value
//!     name: Option<String>,     // None = unchanged, Some(v) = new value
//! }
//! ```
//!
//! Unchanged fields are `None` and skipped during serialization. Only changed
//! fields appear in the JSON.
//!
//! ## `#[deltable(replace)]` — whole-value replacement
//!
//! For types where sub-field diffing doesn't make sense (enums, small structs,
//! types that should always be replaced as a unit), use `#[deltable(replace)]`:
//!
//! ```rust,ignore
//! #[derive(Deltable, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
//! #[deltable(replace)]
//! pub enum SidebarPanel {
//!     None,
//!     Simulation,
//!     GraphInfo,
//! }
//! ```
//!
//! This generates `Delta = Self` — the delta is just the full replacement value.
//! No companion `*Delta` struct is generated. Works on any type: enums, tuple
//! structs, named structs, unit structs. Requires `PartialEq + Clone`.
//!
//! ## Serialization format
//!
//! - Struct delta fields: `None` → omitted, `Some(delta)` → serialized as the
//!   inner delta value.
//! - `OptionDelta`: `Unchanged` → omitted (via `skip_serializing_if`),
//!   `Cleared` → `{"cleared": true}`, `Set(v)` → `{"set": v}`,
//!   `Changed(d)` → serialized as `d`.
//! - `SetDelta`: `{"added": [...], "removed": [...]}` (empty arrays omitted).
//! - `MapDelta`: `{"added": {...}, "removed": [...], "changed": {...}}` (empty
//!   fields omitted). For replacement value types, `changed` contains the full
//!   new value. For `Deltable` struct values, `changed` contains only the
//!   changed fields.
//!
//! ## Trade-offs
//!
//! - **`Vec<T>` is a replacement type**: No stable element identity → whole-value
//!   replacement. For ordered collections with identity, use `BTreeMap` instead.
//!
//! ## Do / Don't
//!
//! - **Do** use `#[derive(Deltable)]` for structs with field-level changes
//!   (settings, configs, traversal params).
//! - **Do** use `#[deltable(replace)]` for enums and small structs that should
//!   always be replaced as a unit.
//! - **Don't** use it for types with complex identity (like graph nodes with
//!   index remapping — those need custom delta logic).
//! - **Do** rely on blanket impls — no field attributes needed.
//! - **Don't** assume `Vec` diffs are element-level — they're whole-value
//!   replacement.

mod helpers;
mod impls;
mod map_delta;
mod option_delta;
mod set_delta;

// Allow `#[derive(Deltable)]` to work inside this crate: the proc macro
// generates `::unigraph_delta::Deltable`, so we re-export ourselves.
#[cfg(test)]
extern crate self as unigraph_delta;

#[cfg(test)]
mod tests;

pub use helpers::apply_option_delta;
pub use helpers::diff_btreemaps;
pub use helpers::diff_btreeset_mapped;
pub use helpers::diff_option;
pub use helpers::diff_optional_btreemaps;
pub use map_delta::MapDelta;
pub use option_delta::OptionDelta;
pub use set_delta::SetDelta;
pub use unigraph_delta_derive::Deltable;

/// A type that can produce a delta between two instances and apply it.
///
/// The `Delta` associated type captures what changed between `self` and `other`.
/// `derive_delta` returns `None` when the two values are equal (no change).
pub trait Deltable: Sized {
    type Delta: serde::Serialize + serde::de::DeserializeOwned + Clone + std::fmt::Debug;

    /// Compute the delta from `self` to `other`. Returns `None` if equal.
    fn derive_delta(&self, other: &Self) -> Option<Self::Delta>;

    /// Mutate `self` by applying `delta`, consuming the delta.
    ///
    /// Returns `Err` if the delta is invalid (e.g. references keys that don't
    /// exist in the current value).
    fn apply_delta(&mut self, delta: Self::Delta) -> anyhow::Result<()>;
}
