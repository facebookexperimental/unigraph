// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;

/// Collect compiled artifact sizes for each crate in the dependency tree.
///
/// An `.rlib` (Rust library) is the compiled output of a library crate. When you
/// run `cargo build`, each dependency gets compiled into an `.rlib` file stored in
/// `target/debug/deps/`. The size of these files reflects how much compiled code
/// each crate contributes — including generic monomorphizations, inlined functions,
/// and codegen output.
///
/// This is useful for build size analysis: large `.rlib` files indicate crates that
/// produce a lot of compiled code, which directly impacts link times and final
/// binary size. Crates with disproportionately large rlibs relative to their
/// functionality are good candidates for replacement or feature-flag trimming.
///
/// Returns a map from crate name to `.rlib` file size in bytes.
pub fn collect_rlib_sizes(target_dir: &Path) -> Result<BTreeMap<String, f64>> {
    let deps_dir = target_dir.join("debug").join("deps");

    if !deps_dir.exists() {
        anyhow::bail!(
            "target/debug/deps not found at {}. Run `cargo build` first.",
            deps_dir.display()
        );
    }

    let mut sizes: BTreeMap<String, f64> = BTreeMap::new();

    let entries = std::fs::read_dir(&deps_dir)
        .with_context(|| format!("Failed to read {}", deps_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("rlib") {
            continue;
        }

        let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        // rlib filenames look like: libcrate_name-<hash>.rlib
        // Strip the "lib" prefix and the "-<hash>" suffix.
        let name = file_name.strip_prefix("lib").unwrap_or(file_name);
        let name = match name.rfind('-') {
            Some(pos) => &name[..pos],
            None => name,
        };
        // Cargo uses underscores in filenames, but crate names may use hyphens.
        let crate_name = name.replace('_', "-");

        let file_size = entry.metadata()?.len() as f64;

        // Keep the largest rlib if there are duplicates.
        let current = sizes.entry(crate_name).or_insert(0.0);
        if file_size > *current {
            *current = file_size;
        }
    }

    Ok(sizes)
}
