// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Parse the binary data format produced by `next experimental-analyze`.
//!
//! # Binary envelope format
//!
//! Both `modules.data` and `analyze.data` share the same envelope:
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ 4 bytes: JSON header length (BE u32)    │
//! ├─────────────────────────────────────────┤
//! │ N bytes: JSON header (UTF-8 JSON)       │
//! ├─────────────────────────────────────────┤
//! │ Remaining bytes: binary edges section   │
//! └─────────────────────────────────────────┘
//! ```
//!
//! The JSON header is deserialized into a typed struct (`ModulesDataHeader` or
//! `AnalyzeDataHeader`). It contains metadata plus `EdgesDataReference` values
//! that point into the binary tail.
//!
//! # Edge adjacency lists
//!
//! Each `EdgesDataReference { offset, length }` locates a block in the binary
//! section that encodes adjacency lists for all nodes:
//!
//! ```text
//! [u32 BE: num_nodes]
//! [u32 BE × num_nodes: cumulative end-offsets]
//! [u32 BE × total_edges: edge target indices]
//! ```
//!
//! Edges for node `i` = `targets[prev..offsets[i]]`
//! where `prev = if i == 0 { 0 } else { offsets[i-1] }`.
//!
//! Target values are indices into the `modules` array in the JSON header.
//!
//! See `ANALYZE_DATA_FORMAT.md` for the complete specification.
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;

// ── JSON header types ───────────────────────────────────────────────────────

/// A pointer into the binary edges section of the file.
///
/// `offset` is the byte offset from the start of the binary tail (after the
/// JSON header). `length` is the number of bytes in this edges block.
/// A block with `length == 0` means "no edges" (the adjacency list is empty
/// for all nodes).
#[derive(Debug, Deserialize)]
pub struct EdgesDataReference {
    pub offset: u32,
    pub length: u32,
}

/// A single module entry from `modules.data`.
///
/// - `ident`: Full turbopack identifier, e.g.
///   `"[project]/src/Button.tsx [app-client] (ecmascript) <exports>"`.
///   Contains the path plus optional layer, type, and fragment suffixes.
///   Parsed by `module_ident::parse_ident()`.
///
/// - `path`: Just the file path without suffixes, e.g.
///   `"[project]/src/Button.tsx"`. Used to match against size data from
///   `analyze.data` (which uses file paths, not full idents).
#[derive(Debug, Deserialize)]
pub struct AnalyzeModule {
    pub ident: String,
    pub path: String,
}

/// JSON header of `modules.data` — the global module dependency graph.
///
/// `modules` is an ordered array. A module's index in this array is its ID,
/// used by all edge references. The four edge references point into the binary
/// tail and encode adjacency lists:
///
/// - `module_dependencies`: outgoing sync edges (static `import`/`require`)
/// - `async_module_dependencies`: outgoing async edges (dynamic `import()`)
/// - `module_dependents`: incoming sync edges (reverse of dependencies)
/// - `async_module_dependents`: incoming async edges (reverse of async deps)
#[derive(Debug, Deserialize)]
pub struct ModulesDataHeader {
    pub modules: Vec<AnalyzeModule>,
    #[expect(dead_code, reason = "deserialized from binary format but not yet used")]
    pub module_dependents: EdgesDataReference,
    #[expect(dead_code, reason = "deserialized from binary format but not yet used")]
    pub async_module_dependents: EdgesDataReference,
    pub module_dependencies: EdgesDataReference,
    pub async_module_dependencies: EdgesDataReference,
}

/// A source file entry in the directory tree within `analyze.data`.
///
/// Sources form a tree via `parent_source_index`. Each node stores only its
/// own path segment (e.g. `"utils.ts"` or `"src/"`), and the full path is
/// reconstructed by walking up the parent chain:
///
/// ```text
/// source[0] = { parent: null, path: "[project]/" }
/// source[1] = { parent: 0,    path: "src/" }
/// source[2] = { parent: 1,    path: "utils.ts" }
/// → full path: "[project]/src/utils.ts"
/// ```
#[derive(Debug, Deserialize)]
pub struct AnalyzeSource {
    pub parent_source_index: Option<u32>,
    pub path: String,
}

/// A chunk part: one source file's contribution to one output chunk.
///
/// Says: "source file `source_index` contributed `size` bytes (`compressed_size`
/// compressed) to output file `output_file_index`."
///
/// The same source can appear in multiple chunk parts (contributing to multiple
/// output files), and sizes are derived from source map attribution — they
/// reflect actual compiled output size, not source size.
#[derive(Debug, Deserialize)]
pub struct AnalyzeChunkPart {
    pub source_index: u32,
    #[allow(dead_code)]
    pub output_file_index: u32,
    pub size: u32,
    pub compressed_size: u32,
}

/// JSON header of per-route `analyze.data` — size attribution for one route.
///
/// Contains NO dependency edges. Only tells you which source files contribute
/// how many bytes to the route's output chunks. The dependency graph comes
/// from `modules.data` instead.
///
/// - `sources`: directory tree of source files (see `AnalyzeSource`)
/// - `chunk_parts`: per-source size contributions (see `AnalyzeChunkPart`)
/// - `source_roots`: indices into `sources` for the tree roots
#[derive(Debug, Deserialize)]
pub struct AnalyzeDataHeader {
    pub sources: Vec<AnalyzeSource>,
    pub chunk_parts: Vec<AnalyzeChunkPart>,
    #[allow(dead_code)]
    pub source_roots: Vec<u32>,
}

// ── Parsed data containers ──────────────────────────────────────────────────

/// Parsed `modules.data` — the global module dependency graph.
///
/// Contains the typed JSON header (module list + edge references) and the raw
/// binary tail (edge adjacency lists). Use `edges_for()` to decode edges for
/// a specific module.
///
/// This is the ONLY source of dependency edges. The per-route `analyze.data`
/// files contain sizes but no edges.
pub struct ModulesData {
    pub header: ModulesDataHeader,
    pub(crate) binary: Vec<u8>,
}

/// Parsed per-route `analyze.data` — size attribution for one route.
///
/// Contains the typed JSON header with source tree and chunk parts.
/// The binary tail is discarded since we only need the JSON fields
/// (`chunk_parts` for sizes, `sources` for path reconstruction).
pub struct AnalyzeData {
    pub header: AnalyzeDataHeader,
}

// ── Loading ─────────────────────────────────────────────────────────────────

/// Read and parse `modules.data` from the given data directory.
pub fn load_modules_data(data_dir: &Path) -> Result<ModulesData> {
    let path = data_dir.join("modules.data");
    let bytes = fs::read(&path).context("failed to read modules.data")?;
    ModulesData::from_bytes(&bytes)
}

// ── Parsing ─────────────────────────────────────────────────────────────────

fn parse_envelope<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<(T, Vec<u8>)> {
    if bytes.len() < 4 {
        bail!("file too short (< 4 bytes)");
    }
    let json_len = read_u32_be(bytes, 0) as usize;
    let json_end = 4 + json_len;
    if bytes.len() < json_end {
        bail!(
            "file too short for JSON header ({json_len} bytes, file has {})",
            bytes.len()
        );
    }
    let header: T =
        serde_json::from_slice(&bytes[4..json_end]).context("failed to parse JSON header")?;
    let binary = bytes[json_end..].to_vec();
    Ok((header, binary))
}

impl ModulesData {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let (header, binary) =
            parse_envelope::<ModulesDataHeader>(bytes).context("parsing modules.data envelope")?;
        Ok(Self { header, binary })
    }

    /// Read the adjacency list for a single node from an edges block.
    pub fn edges_for(&self, reference: &EdgesDataReference, index: usize) -> Vec<u32> {
        read_edges_at(&self.binary, reference, index)
    }
}

impl AnalyzeData {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let (header, _binary) =
            parse_envelope::<AnalyzeDataHeader>(bytes).context("parsing analyze.data envelope")?;
        Ok(Self { header })
    }

    /// Reconstruct the full path for a source by walking up the parent chain.
    pub fn full_source_path(&self, index: usize) -> String {
        let source = &self.header.sources[index];
        match source.parent_source_index {
            None => source.path.clone(),
            Some(parent) => {
                let parent_path = self.full_source_path(parent as usize);
                format!("{parent_path}{}", source.path)
            }
        }
    }
}

// ── Edge decoding ───────────────────────────────────────────────────────────

fn read_edges_at(binary: &[u8], reference: &EdgesDataReference, index: usize) -> Vec<u32> {
    if reference.length == 0 {
        return Vec::new();
    }
    let base = reference.offset as usize;
    let num_nodes = read_u32_be(binary, base) as usize;
    if index >= num_nodes {
        return Vec::new();
    }

    let offsets_start = base + 4;
    let prev_offset = if index == 0 {
        0
    } else {
        read_u32_be(binary, offsets_start + (index - 1) * 4) as usize
    };
    let current_offset = read_u32_be(binary, offsets_start + index * 4) as usize;
    let edge_count = current_offset - prev_offset;
    if edge_count == 0 {
        return Vec::new();
    }

    let data_start = offsets_start + num_nodes * 4;
    (0..edge_count)
        .map(|j| read_u32_be(binary, data_start + (prev_offset + j) * 4))
        .collect()
}

fn read_u32_be(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_u32_be() {
        let data = [0x00, 0x00, 0x00, 0x05];
        assert_eq!(read_u32_be(&data, 0), 5);
    }

    #[test]
    fn test_parse_envelope_minimal() {
        let json = serde_json::json!({
            "modules": [],
            "module_dependents": {"offset": 0, "length": 0},
            "async_module_dependents": {"offset": 0, "length": 0},
            "module_dependencies": {"offset": 0, "length": 0},
            "async_module_dependencies": {"offset": 0, "length": 0},
        });
        let json_bytes = serde_json::to_vec(&json).unwrap();
        let mut blob = (json_bytes.len() as u32).to_be_bytes().to_vec();
        blob.extend_from_slice(&json_bytes);

        let data = ModulesData::from_bytes(&blob).unwrap();
        assert!(data.header.modules.is_empty());
    }

    #[test]
    fn test_edges_decoding() {
        // Build a modules.data with 3 modules and known edges:
        //   module 0 -> [1, 2]
        //   module 1 -> [2]
        //   module 2 -> []
        let json = serde_json::json!({
            "modules": [
                {"ident": "a (ecmascript)", "path": "a"},
                {"ident": "b (ecmascript)", "path": "b"},
                {"ident": "c (ecmascript)", "path": "c"},
            ],
            "module_dependents": {"offset": 0, "length": 0},
            "async_module_dependents": {"offset": 0, "length": 0},
            "module_dependencies": {"offset": 0, "length": 40},
            "async_module_dependencies": {"offset": 0, "length": 0},
        });
        let json_bytes = serde_json::to_vec(&json).unwrap();
        let mut blob = (json_bytes.len() as u32).to_be_bytes().to_vec();
        blob.extend_from_slice(&json_bytes);

        // Build binary edges section:
        // num_nodes = 3
        // offsets = [2, 3, 3]  (cumulative: node0 has 2 edges, node1 has 1, node2 has 0)
        // targets = [1, 2, 2]
        let mut edges_binary: Vec<u8> = Vec::new();
        edges_binary.extend_from_slice(&3u32.to_be_bytes()); // num_nodes
        edges_binary.extend_from_slice(&2u32.to_be_bytes()); // offset[0] = 2
        edges_binary.extend_from_slice(&3u32.to_be_bytes()); // offset[1] = 3
        edges_binary.extend_from_slice(&3u32.to_be_bytes()); // offset[2] = 3
        edges_binary.extend_from_slice(&1u32.to_be_bytes()); // target: 1
        edges_binary.extend_from_slice(&2u32.to_be_bytes()); // target: 2
        edges_binary.extend_from_slice(&2u32.to_be_bytes()); // target: 2
        blob.extend_from_slice(&edges_binary);

        let data = ModulesData::from_bytes(&blob).unwrap();
        assert_eq!(
            data.edges_for(&data.header.module_dependencies, 0),
            vec![1, 2]
        );
        assert_eq!(data.edges_for(&data.header.module_dependencies, 1), vec![2]);
        assert_eq!(
            data.edges_for(&data.header.module_dependencies, 2),
            Vec::<u32>::new()
        );
    }
}
