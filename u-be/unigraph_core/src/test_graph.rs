// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;

use crate::types::MapGraph;

const TEST_GRAPH: &str = r#"
{
    "nodes": {
        "A": {
            "edges": {"directed": ["B", "C"]},
            "size": 1
        },
        "B": {
            "edges": {"directed": ["A", "C"]},
            "size": 2
        },
        "C": {
            "edges": {"directed": ["A", "B", "D"]},
            "size": 3
        },
        "D": {
            "edges": {"directed": ["E", "F"]},
            "size": 4
        },
        "E": {
            "edges": {"directed": ["D", "F"]},
            "size": 5
        },
        "F": {
            "edges": {"directed": ["D", "E"]},
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
        make_test_graph().unwrap();
    }
}
