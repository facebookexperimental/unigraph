// Copyright (c) Meta Platforms, Inc. and affiliates.

/// Read `routes.json` and all per-route `analyze.data` files.
///
/// Produces aggregated sizes (summed across routes) and route membership
/// (which routes each source file appears in) in a single pass per file.
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;

use crate::binary_format::AnalyzeData;

/// Aggregated size for a single source file.
#[derive(Debug, Clone, Default)]
pub struct ModuleSize {
    pub size: u64,
    pub compressed_size: u64,
}

/// Combined route analysis results.
pub struct RouteData {
    /// Aggregated sizes keyed by full source path.
    pub sizes: HashMap<String, ModuleSize>,
    /// For each source path, the set of routes it appears in.
    pub route_membership: HashMap<String, BTreeSet<String>>,
}

pub fn load_route_data(data_dir: &Path) -> Result<RouteData> {
    let routes = read_routes(data_dir)?;
    let mut sizes: HashMap<String, ModuleSize> = HashMap::new();
    let mut route_membership: HashMap<String, BTreeSet<String>> = HashMap::new();

    for route in &routes {
        let analyze_path = route_to_analyze_path(data_dir, route);
        if !analyze_path.exists() {
            continue;
        }
        let bytes = fs::read(&analyze_path)
            .with_context(|| format!("reading {}", analyze_path.display()))?;
        let data = AnalyzeData::from_bytes(&bytes)?;
        process_route(&data, route, &mut sizes, &mut route_membership);
    }

    Ok(RouteData {
        sizes,
        route_membership,
    })
}

fn read_routes(data_dir: &Path) -> Result<Vec<String>> {
    let routes_path = data_dir.join("routes.json");
    if !routes_path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&routes_path).context("failed to read routes.json")?;
    let routes: Vec<String> =
        serde_json::from_str(&contents).context("failed to parse routes.json")?;
    Ok(routes)
}

fn route_to_analyze_path(data_dir: &Path, route: &str) -> std::path::PathBuf {
    if route == "/" {
        data_dir.join("analyze.data")
    } else {
        let stripped = route.strip_prefix('/').unwrap_or(route);
        data_dir.join(stripped).join("analyze.data")
    }
}

/// Process a single route's analyze.data: record route membership and merge
/// sizes using max (not sum) across routes.
///
/// Within one route a module can contribute to multiple output chunks, so we
/// first sum within the route, then take the max across routes. This prevents
/// inflating sizes when the same module appears in many routes' shared chunks.
fn process_route(
    data: &AnalyzeData,
    route: &str,
    sizes: &mut HashMap<String, ModuleSize>,
    route_membership: &mut HashMap<String, BTreeSet<String>>,
) {
    // Phase 1: sum chunk contributions within this route.
    let mut route_sizes: HashMap<String, ModuleSize> = HashMap::new();
    for chunk_part in &data.header.chunk_parts {
        let full_path = data.full_source_path(chunk_part.source_index as usize);
        if full_path.is_empty() {
            continue;
        }

        let entry = route_sizes.entry(full_path.clone()).or_default();
        entry.size += chunk_part.size as u64;
        entry.compressed_size += chunk_part.compressed_size as u64;

        route_membership
            .entry(full_path)
            .or_default()
            .insert(route.to_string());
    }

    // Phase 2: merge into global sizes with max, not sum.
    for (path, route_size) in route_sizes {
        let entry = sizes.entry(path).or_default();
        entry.size = entry.size.max(route_size.size);
        entry.compressed_size = entry.compressed_size.max(route_size.compressed_size);
    }
}
