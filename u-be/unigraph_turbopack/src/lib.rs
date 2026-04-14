// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Convert Next.js Turbopack bundle analysis data into Unigraph `MapGraph` JSON.
//!
//! # Background
//!
//! Next.js ships a command `next experimental-analyze -o` that runs a production
//! Turbopack build and writes binary analysis data to `.next/diagnostics/analyze/data/`.
//! This data contains the full module dependency graph plus per-route size attribution.
//!
//! This crate reads that data and produces a Unigraph `MapGraph` — a JSON format
//! that Unigraph Explorer can visualize with dominator trees, tiered metrics,
//! force-directed layout, and more.
//!
//! # Data sources
//!
//! The analyze output directory contains three kinds of files:
//!
//! - **`modules.data`** — Global module dependency graph. Contains every module
//!   across all routes and RSC layers, plus sync and async dependency edges.
//!   This is ONE graph — edges do not vary per route.
//!
//! - **`routes.json`** — JSON array of route path strings (e.g. `["/", "/about"]`).
//!
//! - **Per-route `analyze.data`** — Size attribution data. For each route, which
//!   source files contribute how many bytes to the route's output chunks. Contains
//!   NO dependency edges — only sizes.
//!
//! See `ANALYZE_DATA_FORMAT.md` in this crate for the full binary format specification.
//!
//! # Key design decisions
//!
//! - **Layers always separate**: The same file compiled in different RSC layers
//!   (`app-rsc`, `app-client`, `app-ssr`) has genuinely different dependency edges
//!   and produces separate nodes. A `layer` label enables UI filtering.
//!
//! - **Single graph, routes as labels**: Since edges are global, we produce one
//!   graph with `route` labels on each node indicating which routes include it.
//!   Sizes are summed across routes.
//!
//! - **Fragments collapsed by default**: Tree-shaking fragments (`<exports>`,
//!   `<module evaluation>`) are merged back into their parent module unless
//!   `--fragments` is passed. Collapsing unions edges, sums sizes, and removes
//!   self-edges that arise from inter-fragment references.
//!
//! - **Tiered traversal**: Sync imports map to the "eager" tier, async imports
//!   (`import()`) map to the "lazy" tier. This gives Unigraph's tiered metrics:
//!   `size#eager` (initial load), `size#lazy` (total), `size#eager~dominated`
//!   (unique cost per module in the initial bundle).
//!
//! # Usage
//!
//! ```bash
//! # Generate analyze data
//! cd your-next-app && ./node_modules/.bin/next experimental-analyze -o
//!
//! # Convert to Unigraph (via task runner)
//! ut unigraph_turbopack .next/diagnostics/analyze/data/ -o graph.json --pretty
//!
//! # Visualize
//! ut serve -f graph.json
//! ```

mod analyze;
mod binary_format;
mod graph;
mod module_ident;

use std::path::Path;

use anyhow::Result;
use unigraph_core::MapGraph;

/// Controls how the Turbopack data is mapped to Unigraph nodes.
pub struct Options {
    /// When true, keep tree-shaking fragments as separate nodes.
    /// Each fragment gets a `fragment` label (e.g. `"exports"`, `"module evaluation"`).
    ///
    /// When false (default), fragments are collapsed into their parent module:
    /// edges are unioned, sizes are summed, and self-edges (between fragments of
    /// the same module) are removed.
    pub fragments: bool,

    /// When true, assign size metrics to nodes in all RSC layers.
    ///
    /// When false (default), only `app-client` and layerless nodes get sizes.
    /// This makes dominator/transitive metrics answer "what JS ships to the
    /// browser?" — RSC and SSR nodes remain as zero-size structural connectors.
    pub all_layer_sizes: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fragments: false,
            all_layer_sizes: false,
        }
    }
}

/// Read Turbopack analyze data from `data_dir` and produce a Unigraph `MapGraph`.
///
/// `data_dir` should point to `.next/diagnostics/analyze/data/` — the directory
/// containing `modules.data`, `routes.json`, and per-route `analyze.data` files.
///
/// The returned `MapGraph` can be serialized to JSON with `serde_json` and
/// loaded into Unigraph Explorer via `ut serve -f graph.json`.
pub fn build_map_graph(data_dir: &Path, opts: &Options) -> Result<MapGraph> {
    let modules_data = binary_format::load_modules_data(data_dir)?;
    let route_data = analyze::load_route_data(data_dir)?;
    graph::build_map_graph(&modules_data, &route_data, opts)
}
