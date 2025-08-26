// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;

use crate::SerializationFormat;
use crate::SerializationFormatInternal;

/// Struct that represents a value that has been serialized using provided
/// serialization format.
/// This is just a convenient wrapper around the serialized data that can be
/// passed around (and double serialized as part of a larger payload)
#[derive(serde::Deserialize, serde::Serialize, typegen::TypeGen)]
pub struct SerializedStr {
    pub data: String,
    pub format: SerializationFormat,
    /// Optional value of the initial type that was serialized. Used for debugging
    pub type_hint: Option<String>,
}

impl SerializedStr {
    pub fn parse<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        SerializationFormatInternal::from(&self.format).parse_string(&self.data)
    }
}
