// Copyright (c) Meta Platforms, Inc. and affiliates.

//! # Unigraph Serialization
//!
//! This module provides high-performance serialization and deserialization utilities
//! with support for multiple formats including JSON and compressed JSON with ZSTD compression.
//!
//! ## Features
//!
//! - **Multiple serialization formats**: Plain JSON, ZSTD-compressed JSON with Base64 encoding
//! - **Configurable compression levels**: Fast, normal, and best compression ratios
//! - **URL-safe encoding**: Support for URL-safe Base64 encoding without padding
//! - **Enhanced error reporting**: Detailed deserialization errors with path information
//!
//! ## Usage
//!
//! ```rust
//! # use unigraph_serialization::SerializationFormat;
//! # use serde::{Serialize, Deserialize};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! #[derive(Serialize, Deserialize)]
//! struct MyData {
//!     name: String,
//!     value: i32,
//! }
//!
//! let data = MyData {
//!     name: "example".to_string(),
//!     value: 42,
//! };
//!
//! // Serialize with compression
//! let serialized = SerializationFormat::JsonZstdBase64.to_string(&data)?;
//!
//! // Deserialize
//! let deserialized: MyData = SerializationFormat::JsonZstdBase64.parse_string(&serialized)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Format Comparison
//!
//! - `Json`: Plain JSON, fastest but largest size
//! - `JsonZstdBase64`: Balanced compression/speed tradeoff
//! - `JsonZstdFastBase64`: Fastest compression, moderate size reduction
//! - `JsonZstdBestBase64`: Slowest compression, best size reduction
//! - `JsonZstdBestBase64URLSafeNoPad`: Best compression with URL-safe encoding

use anyhow::Context;
use anyhow::Result;
use base64::Engine;
mod serialized;
mod truncate_str_in_the_middle;

pub use serialized::SerializedStr;

/// ZSTD compression levels with different performance characteristics.
///
/// Higher levels provide better compression at the cost of speed.
#[derive(Clone, Copy)]
#[repr(i32)]
pub enum ZSTDCompressionLevel {
    /// Fast compression with moderate compression ratio (level 1)
    Fast = 1,
    /// Balanced compression speed and ratio (level 8)
    Normal = 8,
    /// Best compression ratio but slowest (level 18)
    Best = 18,
}

/// Simple enum that can be used externally and passed around as a simple serialized string
#[derive(Debug, typegen::TypeGen, serde::Deserialize, serde::Serialize)]
pub enum SerializationFormat {
    /// Plain JSON format - fastest serialization/deserialization but largest size
    Json,
    /// JSON compressed with ZSTD (normal level) and Base64 encoded
    /// Good trade-off between fast compression and better compression ratio
    JsonZstdBase64,
    /// JSON compressed with ZSTD (fast level) and Base64 encoded
    /// Fastest zstd compression level, not great compression ratio
    JsonZstdFastBase64,
    /// JSON compressed with ZSTD (best level) and Base64 encoded
    /// Very slow compression time but better compression ratio
    JsonZstdBestBase64,
    /// JSON compressed with ZSTD and URL-safe Base64 encoded without padding
    JsonZstdBase64URLSafeNoPad,
    JsonZstdBestBase64URLSafeNoPad,
    JsonZstdFastBase64URLSafeNoPad,
}

/// Internal representation of serialization formats with associated compression levels
/// for easier pattern matching.
enum SerializationFormatInternal {
    Json,
    JsonZstdBase64(ZSTDCompressionLevel),
    JsonZstdBase64URLSafeNoPad(ZSTDCompressionLevel),
}

impl From<&SerializationFormat> for SerializationFormatInternal {
    fn from(format: &SerializationFormat) -> Self {
        match format {
            SerializationFormat::Json => SerializationFormatInternal::Json,
            SerializationFormat::JsonZstdBase64 => {
                SerializationFormatInternal::JsonZstdBase64(ZSTDCompressionLevel::Normal)
            }
            SerializationFormat::JsonZstdFastBase64 => {
                SerializationFormatInternal::JsonZstdBase64(ZSTDCompressionLevel::Fast)
            }
            SerializationFormat::JsonZstdBestBase64 => {
                SerializationFormatInternal::JsonZstdBase64(ZSTDCompressionLevel::Best)
            }
            SerializationFormat::JsonZstdBestBase64URLSafeNoPad => {
                SerializationFormatInternal::JsonZstdBase64URLSafeNoPad(ZSTDCompressionLevel::Best)
            }
            SerializationFormat::JsonZstdBase64URLSafeNoPad => {
                SerializationFormatInternal::JsonZstdBase64URLSafeNoPad(
                    ZSTDCompressionLevel::Normal,
                )
            }
            SerializationFormat::JsonZstdFastBase64URLSafeNoPad => {
                SerializationFormatInternal::JsonZstdBase64URLSafeNoPad(ZSTDCompressionLevel::Fast)
            }
        }
    }
}

impl From<&SerializationFormatInternal> for SerializationFormat {
    fn from(value: &SerializationFormatInternal) -> Self {
        match value {
            SerializationFormatInternal::Json => SerializationFormat::Json,
            SerializationFormatInternal::JsonZstdBase64(level) => match level {
                ZSTDCompressionLevel::Normal => SerializationFormat::JsonZstdBase64,
                ZSTDCompressionLevel::Fast => SerializationFormat::JsonZstdFastBase64,
                ZSTDCompressionLevel::Best => SerializationFormat::JsonZstdBestBase64,
            },
            SerializationFormatInternal::JsonZstdBase64URLSafeNoPad(level) => match level {
                ZSTDCompressionLevel::Best => SerializationFormat::JsonZstdBestBase64URLSafeNoPad,
                ZSTDCompressionLevel::Fast => SerializationFormat::JsonZstdBestBase64URLSafeNoPad,
                ZSTDCompressionLevel::Normal => SerializationFormat::JsonZstdBestBase64URLSafeNoPad,
            },
        }
    }
}

impl SerializationFormat {
    /// Serialize a value to bytes using the specified format.
    ///
    /// # Examples
    /// ```rust
    /// # use unigraph_serialization::SerializationFormat;
    /// let data = vec![1, 2, 3];
    /// let bytes = SerializationFormat::JsonZstdBase64.to_bytes(&data)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn to_bytes<T: serde::Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        SerializationFormatInternal::from(self).to_bytes(value)
    }

    /// Serialize a value to a string using the specified format.
    ///
    /// # Examples
    /// ```rust
    /// # use unigraph_serialization::SerializationFormat;
    /// let data = "hello world";
    /// let serialized = SerializationFormat::Json.to_string(&data)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn to_string<T: serde::Serialize>(&self, value: &T) -> Result<String> {
        SerializationFormatInternal::from(self).to_string(value)
    }

    pub fn to_serialized_str<T: serde::Serialize>(
        &self,
        value: &T,
        type_hint: Option<String>,
    ) -> Result<SerializedStr> {
        SerializationFormatInternal::from(self).to_serialized_str(value, type_hint)
    }

    /// Deserialize a value from bytes using the specified format.
    ///
    /// # Examples
    /// ```rust
    /// # use unigraph_serialization::SerializationFormat;
    /// let json_bytes = b"[1,2,3]";
    /// let data: Vec<i32> = SerializationFormat::Json.parse_bytes(json_bytes)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn parse_bytes<T: serde::de::DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        SerializationFormatInternal::from(self).parse_bytes(bytes)
    }

    /// Deserialize a value from a string using the specified format.
    ///
    /// # Examples
    /// ```rust
    /// # use unigraph_serialization::SerializationFormat;
    /// let json_str = r#"{"name":"test","value":42}"#;
    /// let data: serde_json::Value = SerializationFormat::Json.parse_string(json_str)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn parse_string<T: serde::de::DeserializeOwned>(&self, s: &str) -> Result<T> {
        SerializationFormatInternal::from(self).parse_string(s)
    }
}

impl SerializationFormatInternal {
    pub fn to_bytes<T: serde::Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        match self {
            SerializationFormatInternal::Json => Ok(serde_json::to_vec(value)?),
            SerializationFormatInternal::JsonZstdBase64(level) => {
                let json = serde_json::to_vec(value)?;
                Ok(to_zstd_base64(&json, *level)?.into_bytes())
            }
            SerializationFormatInternal::JsonZstdBase64URLSafeNoPad(level) => {
                let json = serde_json::to_vec(value)?;
                Ok(to_zstd_base64_url_safe_no_pad(&json, *level)?.into_bytes())
            }
        }
    }

    pub fn to_string<T: serde::Serialize>(&self, value: &T) -> Result<String> {
        match self {
            SerializationFormatInternal::Json => Ok(serde_json::to_string(value)?),
            SerializationFormatInternal::JsonZstdBase64(level) => {
                let json = serde_json::to_vec(value)?;
                to_zstd_base64(&json, *level)
            }
            SerializationFormatInternal::JsonZstdBase64URLSafeNoPad(level) => {
                let json = serde_json::to_vec(value)?;
                to_zstd_base64_url_safe_no_pad(&json, *level)
            }
        }
    }

    pub fn to_serialized_str<T: serde::Serialize>(
        &self,
        value: &T,
        type_hint: Option<String>,
    ) -> Result<SerializedStr> {
        let data = self.to_string(value)?;
        Ok(SerializedStr {
            data,
            format: self.into(),
            type_hint,
        })
    }

    pub fn parse_bytes<T: serde::de::DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        match self {
            SerializationFormatInternal::Json => de_json_bytes_path_to_error!(bytes, T),
            SerializationFormatInternal::JsonZstdBase64(..)
            | SerializationFormatInternal::JsonZstdBase64URLSafeNoPad(..) => {
                anyhow::bail!("For base64 formats deserialization, use from_string instead")
            }
        }
    }

    pub fn parse_string<T: serde::de::DeserializeOwned>(&self, s: &str) -> Result<T> {
        match self {
            SerializationFormatInternal::Json => de_json_path_to_error!(s, T),
            SerializationFormatInternal::JsonZstdBase64(_) => {
                let decompressed = from_zstd_base64(s)?;
                de_json_bytes_path_to_error!(&decompressed, T)
            }
            SerializationFormatInternal::JsonZstdBase64URLSafeNoPad(_) => {
                let decompressed = from_zstd_base64_url_safe_no_pad(s)?;
                de_json_bytes_path_to_error!(&decompressed, T)
            }
        }
    }
}

fn from_zstd_base64(zstd_base64: &str) -> Result<Vec<u8>> {
    let compressed = from_base64(zstd_base64)?;
    from_zstd(&compressed[..])
}

pub fn from_zstd(bytes: &[u8]) -> Result<Vec<u8>> {
    let decompressed = zstd::decode_all(bytes).context("Failed to decompress zstd data")?;
    Ok(decompressed)
}

pub fn to_zstd(bytes: &[u8], level: ZSTDCompressionLevel) -> Result<Vec<u8>> {
    let compressed =
        zstd::encode_all(bytes, level as i32).context("Failed to compress data with zstd")?;
    Ok(compressed)
}

pub fn to_base_64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn from_base64(s: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .context("Failed to decode base64 string")
}

fn to_zstd_base64_url_safe_no_pad(data: &[u8], level: ZSTDCompressionLevel) -> Result<String> {
    let compressed = to_zstd(data, level)?;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed);
    Ok(encoded)
}

fn to_zstd_base64(data: &[u8], level: ZSTDCompressionLevel) -> Result<String> {
    let compressed = to_zstd(data, level)?;
    let encoded = to_base_64(&compressed);
    Ok(encoded)
}

fn from_zstd_base64_url_safe_no_pad(zstd_base64: &str) -> Result<Vec<u8>> {
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(zstd_base64)
        .context("Failed to decode base64 URL-safe string (no padding)")?;
    from_zstd(&compressed[..])
}

/// Deserialize JSON but give a bit more descriptive deserialization error
/// providing the full path to the value that failed deserialization instead of
/// just blowing up on "unexpected string"
/// Example:
/// ```text
/// struct MyTestStruct {
///     a: usize,
/// }
///
/// let json = r#"{"field": "value"}"#;
/// let result: anyhow::Result<MyStruct> = de_json_path_to_error!(json, MyStruct);
/// ```
///
/// If deserialization fails, the error will include:
/// - The full JSON content (truncated if too long)
/// - The target type name
/// - The specific path where the error occurred
/// - The detailed error description
#[macro_export]
macro_rules! de_json_path_to_error {
    ( $json:expr_2021, $t:ty ) => {{
        let result: anyhow::Result<$t> =
            $crate::__de_json_with_path_to_error($json, stringify!($t));
        result
    }};
}

/// Deserialize JSON from bytes with enhanced error reporting.
///
/// Similar to `de_json_path_to_error!` but works with byte slices instead of strings.
/// Provides detailed deserialization errors including the full path to the value
/// that failed deserialization.
///
/// # Arguments
/// * `$json` - The JSON byte slice to deserialize
/// * `$t` - The target type to deserialize into
///
/// # Returns
/// * `Result<T>` - The deserialized value or an error with path information
#[macro_export]
macro_rules! de_json_bytes_path_to_error {
    ( $json:expr_2021, $t:ty ) => {{
        let result: anyhow::Result<$t> =
            $crate::__de_json_bytes_with_path_to_error($json, stringify!($t));
        result
    }};
}

/// Internal function for deserializing JSON from bytes with path-aware error reporting.
///
/// This function is used by the `de_json_bytes_path_to_error!` macro and provides
/// enhanced error messages that include the JSON content and the path to the error.
pub fn __de_json_bytes_with_path_to_error<T: serde::de::DeserializeOwned>(
    json_bytes: &[u8],
    t: &str,
) -> Result<T> {
    let json_de = &mut serde_json::Deserializer::from_slice(json_bytes);
    let result: Result<T, _> = serde_path_to_error::deserialize(json_de);

    result.map_err(|e| {
        anyhow::anyhow!(
            "
Error while parsing JSON.

JSON:
============================================
{}
============================================

Deserialization type: `{}`
Error: `{:?}`
Path to error: `{}`
",
            truncate_str_in_the_middle::truncate_str_in_the_middle(
                &String::from_utf8_lossy(json_bytes),
                5000
            ),
            t,
            e,
            e.path(),
        )
    })
}

/// Internal function for deserializing JSON from strings with path-aware error reporting.
pub fn __de_json_with_path_to_error<T: serde::de::DeserializeOwned>(
    json: &str,
    t: &str,
) -> Result<T> {
    __de_json_bytes_with_path_to_error(json.as_bytes(), t)
}

#[cfg(test)]
mod tests {
    use k9::assert_equal;

    use super::*;

    #[test]
    fn serialize_plain_string() -> Result<()> {
        let original = "Hello, world!";
        let s = SerializationFormat::JsonZstdBestBase64URLSafeNoPad.to_string(&original)?;
        let roundtrip: String =
            SerializationFormat::JsonZstdBestBase64URLSafeNoPad.parse_string(&s)?;
        assert_equal!(original, roundtrip);
        Ok(())
    }
}
