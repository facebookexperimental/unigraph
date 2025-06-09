use anyhow::Context;
use anyhow::Result;
use base64::Engine;

pub fn from_zstd_base64(zstd_base64: &str) -> Result<Vec<u8>> {
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(zstd_base64)
        .context("Failed to decode base64 string")?;

    let decompressed =
        zstd::decode_all(&compressed[..]).context("Failed to decompress zstd data")?;
    Ok(decompressed)
}

pub fn to_zstd_base64_url_safe_no_pad(data: &[u8], level: i32) -> Result<String> {
    let compressed = zstd::encode_all(data, level).context("Failed to compress data with zstd")?;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed);
    Ok(encoded)
}

pub fn from_zstd_base64_url_safe_no_pad(zstd_base64: &str) -> Result<Vec<u8>> {
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(zstd_base64)
        .context("Failed to decode base64 URL-safe string (no padding)")?;

    let decompressed =
        zstd::decode_all(&compressed[..]).context("Failed to decompress zstd data")?;
    Ok(decompressed)
}
