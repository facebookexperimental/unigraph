// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::Context;
use anyhow::Result;

/// Build timing info for a single compilation unit.
pub struct UnitTiming {
    pub duration: f32,
    pub rmeta_time: f32,
    pub codegen_time: f32,
}

/// Run `cargo build --timings=json` and parse the resulting timing data.
/// Returns a map from crate name to timing info.
///
/// With `--timings=json`, cargo emits one JSON object per line to stdout,
/// each with `"reason":"timing-info"` containing per-crate build timing.
pub fn collect_timings(manifest_path: &Path) -> Result<BTreeMap<String, UnitTiming>> {
    // --timings=json is unstable, so we need nightly + -Z unstable-options.
    let output = Command::new("cargo")
        .args([
            "+nightly",
            "build",
            "--timings=json",
            "-Z",
            "unstable-options",
        ])
        .arg("--manifest-path")
        .arg(manifest_path)
        .output()
        .context("Failed to run cargo +nightly build --timings=json")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo build --timings=json failed:\n{stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_timing_lines(&stdout)
}

fn parse_timing_lines(output: &str) -> Result<BTreeMap<String, UnitTiming>> {
    let mut timings = BTreeMap::new();

    for line in output.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if value["reason"].as_str() != Some("timing-info") {
            continue;
        }

        let Some(name) = value["target"]["name"].as_str() else {
            continue;
        };

        let duration = value["duration"].as_f64().unwrap_or(0.0) as f32;
        let rmeta_time = value["rmeta_time"].as_f64().unwrap_or(0.0) as f32;
        // codegen_time is not always present; compute from duration - rmeta_time.
        let codegen_time = if value.get("codegen_time").is_some() {
            value["codegen_time"].as_f64().unwrap_or(0.0) as f32
        } else {
            (duration - rmeta_time).max(0.0)
        };

        // Use the crate name as key. If there are duplicates (e.g. build script
        // vs lib), keep the one with the longer duration.
        let entry = timings.entry(name.to_string()).or_insert(UnitTiming {
            duration: 0.0,
            rmeta_time: 0.0,
            codegen_time: 0.0,
        });
        if duration > entry.duration {
            entry.duration = duration;
            entry.rmeta_time = rmeta_time;
            entry.codegen_time = codegen_time;
        }
    }

    Ok(timings)
}
