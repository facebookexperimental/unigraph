use std::collections::BTreeSet;

use anyhow::Result;

use crate::ArrayGraph;
use crate::ArrayGraphSerializable;
use crate::NodeIDX;
use crate::remap_utils::RemapContext;

pub(crate) fn get_reachable_subgraph_unconfigured(
    graph: ArrayGraph,
    roots: &[NodeIDX],
) -> Result<ArrayGraphSerializable> {
    let mut reachable = vec![false; graph.nodes_len()];
    let mut total_reachable = 0;

    let root_names = roots
        .iter()
        .map(|&idx| graph.idx_to_name(idx).to_string())
        .collect::<BTreeSet<_>>();

    for node_idx in graph.forward_edge_view().dfs_unconfigured(roots) {
        reachable[node_idx] = true;
        total_reachable += 1;
    }

    let mut names = String::new();
    let mut offsets = vec![0];
    let mut remap_ctx = RemapContext {
        original_positions: Vec::with_capacity(total_reachable),
        mappings: Vec::with_capacity(graph.nodes_len()),
    };

    for (node_idx, &reachable) in reachable.iter().enumerate() {
        if reachable {
            let name = graph.idx_to_name(node_idx);
            names.push_str(name);
            offsets.push(names.len());

            let new_node_idx = remap_ctx.original_positions.len();
            remap_ctx.original_positions.push(Some(node_idx.into()));
            remap_ctx.mappings.push(Some(new_node_idx.into()));
        } else {
            remap_ctx.mappings.push(None);
        }
    }

    let gs = graph.into_serializable();
    let mut remapped = gs.remap(&remap_ctx)?;

    remapped.entry_points = Some(root_names);

    Ok(remapped)
}
