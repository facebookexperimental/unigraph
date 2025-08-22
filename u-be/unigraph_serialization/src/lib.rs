// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;
use base64::Engine;
mod truncate_str_in_the_middle;
const ZSTD_LEVEL_NORMAL: i32 = 8;

#[derive(typegen::TypeGen, serde::Deserialize, serde::Serialize)]
pub enum SerializationFormat {
    Json,
    JsonZstdBase64,
    JsonZstdBase64URLSafeNoPad,
}

impl SerializationFormat {
    pub fn to_bytes<T: serde::Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        match self {
            SerializationFormat::Json => Ok(serde_json::to_vec(value)?),
            SerializationFormat::JsonZstdBase64 => {
                let json = serde_json::to_vec(value)?;
                Ok(to_zstd_base64(&json, ZSTD_LEVEL_NORMAL)?.into_bytes())
            }
            SerializationFormat::JsonZstdBase64URLSafeNoPad => {
                let json = serde_json::to_vec(value)?;
                Ok(to_zstd_base64_url_safe_no_pad(&json, ZSTD_LEVEL_NORMAL)?.into_bytes())
            }
        }
    }

    pub fn to_string<T: serde::Serialize>(&self, value: &T) -> Result<String> {
        match self {
            SerializationFormat::Json => Ok(serde_json::to_string(value)?),
            SerializationFormat::JsonZstdBase64 => {
                let json = serde_json::to_vec(value)?;
                to_zstd_base64(&json, ZSTD_LEVEL_NORMAL)
            }
            SerializationFormat::JsonZstdBase64URLSafeNoPad => {
                let json = serde_json::to_vec(value)?;
                to_zstd_base64_url_safe_no_pad(&json, ZSTD_LEVEL_NORMAL)
            }
        }
    }

    pub fn from_bytes<T: serde::de::DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        match self {
            SerializationFormat::Json => de_json_bytes_path_to_error!(bytes, T),
            SerializationFormat::JsonZstdBase64
            | SerializationFormat::JsonZstdBase64URLSafeNoPad => {
                anyhow::bail!("For base64 formats deserialization, use from_string instead")
            }
        }
    }

    pub fn from_string<T: serde::de::DeserializeOwned>(&self, s: &str) -> Result<T> {
        match self {
            SerializationFormat::Json => de_json_path_to_error!(s, T),
            SerializationFormat::JsonZstdBase64 => {
                let decompressed = from_zstd_base64(s)?;
                de_json_bytes_path_to_error!(&decompressed, T)
            }
            SerializationFormat::JsonZstdBase64URLSafeNoPad => {
                let decompressed = from_zstd_base64_url_safe_no_pad(s)?;
                de_json_bytes_path_to_error!(&decompressed, T)
            }
        }
    }
}

fn from_zstd_base64(zstd_base64: &str) -> Result<Vec<u8>> {
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(zstd_base64)
        .context("Failed to decode base64 string")?;

    let decompressed =
        zstd::decode_all(&compressed[..]).context("Failed to decompress zstd data")?;
    Ok(decompressed)
}

fn to_zstd_base64_url_safe_no_pad(data: &[u8], level: i32) -> Result<String> {
    let compressed = zstd::encode_all(data, level).context("Failed to compress data with zstd")?;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed);
    Ok(encoded)
}

fn to_zstd_base64(data: &[u8], level: i32) -> Result<String> {
    let compressed = zstd::encode_all(data, level).context("Failed to compress data with zstd")?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(compressed);
    Ok(encoded)
}

fn from_zstd_base64_url_safe_no_pad(zstd_base64: &str) -> Result<Vec<u8>> {
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(zstd_base64)
        .context("Failed to decode base64 URL-safe string (no padding)")?;

    let decompressed =
        zstd::decode_all(&compressed[..]).context("Failed to decompress zstd data")?;
    Ok(decompressed)
}

/// Deserialize JSON but give a bit more descriptive deserialization error
/// providing the full path to the value that failed deserialization instead of
/// just blowing up on "unexpected string"
/// Example:
/// ```text
/// struct MyTestStruct {
///     a: usize,
/// }
/// let json = r#"  {"a": 1}  "#;
/// de_json_path_to_error!(json, MyTestStruct);
/// ```
#[macro_export]
macro_rules! de_json_path_to_error {
    ( $json:expr_2021, $t:ty ) => {{
        let result: anyhow::Result<$t> =
            $crate::__de_json_with_path_to_error($json, stringify!($t));
        result
    }};
}

#[macro_export]
macro_rules! de_json_bytes_path_to_error {
    ( $json:expr_2021, $t:ty ) => {{
        let result: anyhow::Result<$t> =
            $crate::__de_json_bytes_with_path_to_error($json, stringify!($t));
        result
    }};
}

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
        let s = SerializationFormat::JsonZstdBase64URLSafeNoPad.to_string(&original)?;
        let roundtrip: String = SerializationFormat::JsonZstdBase64URLSafeNoPad.from_string(&s)?;
        assert_equal!(original, roundtrip);
        Ok(())
    }
}
