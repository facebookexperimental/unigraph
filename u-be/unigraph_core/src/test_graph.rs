// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;

use crate::types::MapGraph;

const TEST_GRAPH: &str = r#"
{
    "nodes": {
        "A": {
            "edges_directed": ["B", "C"],
            "size": 1
        },
        "B": {
            "edges_directed": ["A", "C"],
            "size": 2
        },
        "C": {
            "edges_directed": ["A", "B", "D"],
            "size": 3
        },
        "D": {
            "edges_directed": ["E", "F"],
            "size": 4
        },
        "E": {
            "edges_directed": ["D", "F"],
            "size": 5
        },
        "F": {
            "edges_directed": ["D", "E"],
            "size": 6
        }
    }
}"#;

pub fn make_test_graph() -> Result<MapGraph> {
    serde_json::from_str::<MapGraph>(TEST_GRAPH).context("failed to parse test graph")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_test_graph() {
        let _graph = make_test_graph().unwrap();
    }
}
