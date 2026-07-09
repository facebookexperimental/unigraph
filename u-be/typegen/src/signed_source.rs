// Copyright (c) Meta Platforms, Inc. and affiliates.

//! SignedSource signing for generated files.
//!
//! Files that live in a `__generated__` (or `__db_generated__`) directory must
//! carry a SignedSource signature so that manual edits are detected by lint.
//!
//! [`sign`] takes fully-assembled file content whose header contains a plain
//! `@generated` (or `@partially-generated`) marker, upgrades that marker to
//! `@generated SignedSource<<md5>>`, and returns the signed content. The MD5 is
//! computed over the content with the canonical signing token in place, so the
//! result verifies against the `signedsources` crate.

use md5::Digest;
use md5::Md5;

/// The canonical SignedSource placeholder token. The MD5 is computed with this
/// token sitting where the signature goes, then the token is swapped for
/// `SignedSource<<md5>>`. Must match the token used across Meta tooling.
pub const SIGNING_TOKEN: &str = "<<SignedSource::*O*zOeWoEQle#+L!plEphiEmie@IsG>>";

/// The shape of an already-applied signature, used to avoid double-signing.
const SIGNATURE_PREFIX: &str = " SignedSource<<";

/// Markers that indicate a file should be signed. `@partially-generated` is
/// checked first because `@generated` is otherwise a substring-free match.
const MARKERS: &[&str] = &["@partially-generated", "@generated"];

/// Sign `content` if it carries an unsigned generated marker.
///
/// Returns `content` unchanged when there is no `@generated` /
/// `@partially-generated` marker, or when it is already signed.
pub fn sign(content: String) -> String {
    let with_token = match content.find(SIGNING_TOKEN) {
        Some(_) => content,
        None => match insert_token(&content) {
            Some(c) => c,
            None => return content,
        },
    };

    let signature = format!("SignedSource<<{:x}>>", Md5::digest(with_token.as_bytes()));
    with_token.replace(SIGNING_TOKEN, &signature)
}

/// Insert the [`SIGNING_TOKEN`] after the first unsigned generated marker.
///
/// Returns `None` if no unsigned marker is found.
fn insert_token(content: &str) -> Option<String> {
    for marker in MARKERS {
        let mut search_start = 0;
        while let Some(rel) = content[search_start..].find(marker) {
            let after = search_start + rel + marker.len();
            // Skip markers that already carry a signature.
            if content[after..].starts_with(SIGNATURE_PREFIX) {
                search_start = after;
                continue;
            }

            let mut signed = String::with_capacity(content.len() + SIGNING_TOKEN.len() + 1);
            signed.push_str(&content[..after]);
            signed.push(' ');
            signed.push_str(SIGNING_TOKEN);
            signed.push_str(&content[after..]);
            return Some(signed);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::sign;

    fn extract_md5(signed: &str) -> &str {
        let prefix = "SignedSource<<";
        let start = signed.find(prefix).expect("signature present") + prefix.len();
        &signed[start..start + 32]
    }

    #[test]
    fn test_no_marker_is_noop() {
        let content = "<?hh\n/* hack header */\n\ntype Foo = int;\n".to_string();
        assert_eq!(sign(content.clone()), content);
    }

    #[test]
    fn test_signs_generated_marker() {
        let content = " * @generated\nbody\n".to_string();
        let signed = sign(content);

        assert!(signed.contains(" * @generated SignedSource<<"));
        let hash = extract_md5(&signed);
        assert_eq!(hash.len(), 32);
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn test_signs_partially_generated_marker() {
        let content = "// @partially-generated\nbody\n".to_string();
        let signed = sign(content);
        assert!(signed.contains("// @partially-generated SignedSource<<"));
    }

    #[test]
    fn test_does_not_touch_other_at_tags() {
        let content = " * @flow\n * @generated\n * @codegen-command: foo\n".to_string();
        let signed = sign(content);
        assert!(signed.contains(" * @flow\n"));
        assert!(signed.contains(" * @codegen-command: foo\n"));
        assert!(signed.contains(" * @generated SignedSource<<"));
    }

    #[test]
    fn test_already_signed_is_noop() {
        let content =
            " * \x40generated SignedSource<<00000000000000000000000000000000>>\nbody\n".to_string();
        assert_eq!(sign(content.clone()), content);
    }

    #[test]
    fn test_signature_changes_with_content() {
        let a = sign(" * @generated\npayload a\n".to_string());
        let b = sign(" * @generated\npayload b\n".to_string());
        assert_ne!(extract_md5(&a), extract_md5(&b));
    }
}
