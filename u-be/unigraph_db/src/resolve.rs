// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Graph query config resolution — resolve a [`GraphQueryConfig`] to a
//! fully-prepared [`ArrayGraph`].
//!
//! The resolution pipeline:
//!
//! 1. **Handle** → resolve the graph handle (recursing into GQC keys).
//!    If the handle is a GQC key, fetch the inner GQC for default roots/traversal.
//! 2. **Roots** → `gqc.roots` > inner GQC roots > no filtering.
//! 3. **Traversal** → `gqc.traversal` > inner GQC traversal > graph's own.
//! 4. Extract reachable subgraph from roots (if any).
//! 5. Apply traversal config to the `ArrayGraph`.

use anyhow::Result;
use unigraph_core::ArrayGraph;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::GraphHandle;
use unigraph_core::TraversalConfig;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_storage_core::GraphKey;

use crate::UnigraphDb;

impl UnigraphDb {
    /// Resolve a [`GraphQueryConfig`] to a fully-prepared [`ArrayGraph`].
    ///
    /// Handles the full pipeline: handle resolution (including nested GQC keys),
    /// root filtering, and traversal config application. This is the primary
    /// entry point for "give me the graph described by this query config".
    ///
    /// When `add_super_root` is true and the graph has multiple entry points, a
    /// synthetic super-root is appended *before* traversal so it is tiered and
    /// made reachable by the same traversal pass as every other node.
    pub async fn resolve_graph_query_config(
        &self,
        gqc: &GraphQueryConfig,
        add_super_root: bool,
        task: &ll::Task,
    ) -> Result<(GraphKey, ArrayGraph)> {
        let (inner_gqc, key, ags) = self.resolve_handle_and_fetch(gqc, task).await?;
        let roots = resolve_roots(gqc, &inner_gqc);
        let traversal = self.resolve_gqc_traversal(gqc, &inner_gqc, task).await?;

        let ags = extract_subgraph(ags, &roots, task)?;

        let mut ag = ags.into_array_graph(task)?;
        if add_super_root {
            ag = ag.append_super_root(false)?;
        }
        apply_traversal(&mut ag, traversal.as_ref())?;
        Ok((key, ag))
    }
}

// -- Private helpers ----------------------------------------------------------

impl UnigraphDb {
    /// Resolve the handle: if it's a GQC key, fetch the inner GQC and use its
    /// handle to fetch the graph. Otherwise fetch the graph directly.
    async fn resolve_handle_and_fetch(
        &self,
        gqc: &GraphQueryConfig,
        task: &ll::Task,
    ) -> Result<(Option<GraphQueryConfig>, GraphKey, ArrayGraphSerializable)> {
        match &gqc.handle {
            GraphHandle::GqcKey(gqc_key) => {
                let inner_gqc = self.configs.fetch_graph_query_config(gqc_key, task).await?;
                let (key, ags) = self.fetch_graph_by_handle(&inner_gqc.handle, task).await?;
                Ok((Some(inner_gqc), key, ags))
            }
            handle => {
                let (key, ags) = self.fetch_graph_by_handle(handle, task).await?;
                Ok((None, key, ags))
            }
        }
    }

    /// Resolve the traversal config to apply.
    ///
    /// Priority: `gqc.traversal` > inner GQC traversal > `None` (falls through
    /// to `graph.traversal_config` in [`apply_traversal`]).
    async fn resolve_gqc_traversal(
        &self,
        gqc: &GraphQueryConfig,
        inner_gqc: &Option<GraphQueryConfig>,
        task: &ll::Task,
    ) -> Result<Option<TraversalConfig>> {
        if let Some(t) = &gqc.traversal {
            return Ok(Some(
                self.configs.resolve_traversal_override(t, task).await?,
            ));
        }
        if let Some(inner) = inner_gqc
            && let Some(t) = &inner.traversal
        {
            return Ok(Some(
                self.configs.resolve_traversal_override(t, task).await?,
            ));
        }
        Ok(None)
    }
}

/// Determine which roots to use.
///
/// Priority: `gqc.roots` > inner GQC roots > empty (no filtering).
fn resolve_roots(gqc: &GraphQueryConfig, inner_gqc: &Option<GraphQueryConfig>) -> Vec<String> {
    if let Some(roots) = &gqc.roots {
        return roots.iter().cloned().collect();
    }
    if let Some(inner) = inner_gqc
        && let Some(roots) = &inner.roots
    {
        return roots.iter().cloned().collect();
    }
    Vec::new()
}

/// If roots are specified, extract only the reachable subgraph.
fn extract_subgraph(
    ags: ArrayGraphSerializable,
    roots: &[String],
    task: &ll::Task,
) -> Result<ArrayGraphSerializable> {
    if roots.is_empty() {
        return Ok(ags);
    }
    let ag = ags.into_array_graph(task)?;
    let root_idxs: Vec<_> = roots
        .iter()
        .filter_map(|name| ag.data.node_names_ordered.name_to_idx_log(name.as_str()))
        .collect();
    ag.get_reachable_subgraph_unconfigured(&root_idxs)
}

fn apply_traversal(ag: &mut ArrayGraph, traversal: Option<&TraversalConfig>) -> Result<()> {
    let tvc = traversal.or(ag.runtime.state.traversal_config.as_ref());
    if let Some(tvc) = tvc {
        ag.apply_traversal_config_and_entry_points(tvc.clone())?;
    }
    Ok(())
}
