// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use cargo_metadata::DependencyKind;
use cargo_metadata::MetadataCommand;
use cargo_metadata::Package;
use cargo_metadata::PackageId;

/// Parsed dependency info for a single crate.
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub source: String,
    pub manifest_path: String,
    pub crate_type: String,
    /// Normal (production) dependencies by node name.
    pub normal_deps: Vec<String>,
    /// Dev dependencies by node name.
    pub dev_deps: Vec<String>,
    /// Build dependencies by node name.
    pub build_deps: Vec<String>,
}

pub struct CargoGraph {
    pub crates: BTreeMap<String, CrateInfo>,
    pub workspace_members: Vec<String>,
    pub target_directory: PathBuf,
}

/// Node name for a package: "name v0.1.0" to disambiguate versions.
fn node_name(pkg: &Package) -> String {
    format!("{} v{}", pkg.name, pkg.version)
}

fn classify_source(pkg: &Package, workspace_member_ids: &[PackageId]) -> String {
    if workspace_member_ids.contains(&pkg.id) {
        "workspace".to_string()
    } else if let Some(src) = &pkg.source {
        let repr = src.repr.as_str();
        if repr.starts_with("registry+") {
            "crates.io".to_string()
        } else if repr.starts_with("git+") {
            "git".to_string()
        } else {
            "path".to_string()
        }
    } else {
        "path".to_string()
    }
}

fn primary_crate_type(pkg: &Package) -> String {
    pkg.targets
        .first()
        .and_then(|t| t.kind.first())
        .map(|k| k.to_string())
        .unwrap_or_else(|| "lib".to_string())
}

pub fn collect_metadata(manifest_path: &Path) -> Result<CargoGraph> {
    let metadata = MetadataCommand::new()
        .manifest_path(manifest_path)
        .exec()
        .context("Failed to run cargo metadata")?;

    let resolve = metadata
        .resolve
        .as_ref()
        .context("cargo metadata did not include dependency resolution")?;

    // Build a lookup from PackageId -> Package.
    let pkg_by_id: BTreeMap<&PackageId, &Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();

    let workspace_member_ids: Vec<PackageId> = metadata.workspace_members.clone();

    // Build node name lookup.
    let node_name_by_id: BTreeMap<&PackageId, String> = metadata
        .packages
        .iter()
        .map(|p| (&p.id, node_name(p)))
        .collect();

    let mut crates = BTreeMap::new();

    for resolved_node in &resolve.nodes {
        let Some(pkg) = pkg_by_id.get(&resolved_node.id) else {
            continue;
        };

        let name = node_name(pkg);
        let mut normal_deps = Vec::new();
        let mut dev_deps = Vec::new();
        let mut build_deps = Vec::new();

        for dep in &resolved_node.deps {
            let Some(dep_name) = node_name_by_id.get(&dep.pkg) else {
                continue;
            };

            // A dep can have multiple dep_kinds (e.g. both normal and dev).
            // We pick the "strongest": normal > build > dev.
            let dominant_kind = dep
                .dep_kinds
                .iter()
                .map(|dk| &dk.kind)
                .min_by_key(|k| match k {
                    DependencyKind::Normal => 0,
                    DependencyKind::Build => 1,
                    DependencyKind::Development => 2,
                    _ => 3,
                })
                .cloned()
                .unwrap_or(DependencyKind::Normal);

            match dominant_kind {
                DependencyKind::Normal => normal_deps.push(dep_name.clone()),
                DependencyKind::Build => build_deps.push(dep_name.clone()),
                DependencyKind::Development => dev_deps.push(dep_name.clone()),
                _ => normal_deps.push(dep_name.clone()),
            }
        }

        let info = CrateInfo {
            name: name.clone(),
            version: pkg.version.to_string(),
            source: classify_source(pkg, &workspace_member_ids),
            manifest_path: pkg.manifest_path.to_string(),
            crate_type: primary_crate_type(pkg),
            normal_deps,
            dev_deps,
            build_deps,
        };

        crates.insert(name, info);
    }

    let workspace_members = workspace_member_ids
        .iter()
        .filter_map(|id| node_name_by_id.get(id).cloned())
        .collect();

    Ok(CargoGraph {
        crates,
        workspace_members,
        target_directory: metadata.target_directory.into(),
    })
}
