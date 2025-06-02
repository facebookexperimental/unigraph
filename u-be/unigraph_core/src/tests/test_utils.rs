// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::ArrayGraph;
use crate::types::NodeIDX;

pub fn idx_to_names<I: IntoIterator<Item = NodeIDX>>(graph: &ArrayGraph, idxs: I) -> Vec<String> {
    idxs.into_iter()
        .map(|idx| graph.idx_to_name(idx).to_string())
        .collect()
}
