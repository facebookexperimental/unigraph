// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::AboutGraphInput;
use unigraph_app::call_rpc;

use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;
use crate::support::fixtures::ingest_map_graph_json;

// ── Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn returns_graph_settings() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let out = call_rpc!(
        t,
        AboutGraph(AboutGraphInput {
            handle: handle.parse()?,
        })
    );

    let gs = out
        .graph_settings
        .expect("graph_settings should be present");
    assert!(gs.description.is_none());
    assert!(gs.metrics_config.is_some());

    let visibility = gs.metrics_visibility.as_ref().unwrap();
    let keys: Vec<&str> = visibility.keys().map(|k| k.as_str()).collect();
    snapshot!(keys.join(", "), "size#eager");

    Ok(())
}

#[tokio::test]
async fn returns_properties() -> Result<()> {
    let t = init_app();
    let json = r#"{
        "nodes": {
            "a": { "metrics": { "size": 10 }, "edges_directed": ["b"] },
            "b": { "metrics": { "size": 20 } }
        },
        "properties": {
            "owner": "infra-team",
            "version": "2.1.0"
        }
    }"#;
    ingest_map_graph_json(&t, "props_test", json).await?;

    let out = call_rpc!(
        t,
        AboutGraph(AboutGraphInput {
            handle: "props_test".parse()?,
        })
    );

    snapshot!(
        format!("{:#?}", out.properties),
        r#"
{
    "owner": "infra-team",
    "version": "2.1.0",
}
"#
    );

    Ok(())
}

#[tokio::test]
async fn text_summary() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let out = call_rpc!(
        t,
        AboutGraph(AboutGraphInput {
            handle: handle.parse()?,
        })
    );

    snapshot!(
        out.text,
        "
# Graph: explore_test

## Stats

- **Nodes**: 12
- **Edges**: 17 (13 directed, 2 tagged, 2 dynamic)

## Metrics

- **`lines`** — Lines of code
- **`node_type`** — Kind of module
- **`size`** — Module size in bytes

## All Available Metric Views

- `lines`
- `lines~transitive`
- `lines~dominated`
- `node_type`
- `size~transitive`
- `size~dominated`
- `size#eager`
- `size#eager~dominated`
- `size#lazy`
- `size#lazy~dominated`
- `parents-count`
- `node-count~transitive`
- `node-count~dominated`
- `tier`

## Tiers

- eager
- lazy

"
    );

    Ok(())
}

#[tokio::test]
async fn text_summary_with_properties() -> Result<()> {
    let t = init_app();
    let json = r#"{
        "nodes": {
            "a": { "metrics": { "size": 10 }, "edges_directed": ["b"] },
            "b": { "metrics": { "size": 20 } }
        },
        "properties": {
            "owner": "infra-team",
            "version": "2.1.0"
        }
    }"#;
    ingest_map_graph_json(&t, "props_text_test", json).await?;

    let out = call_rpc!(
        t,
        AboutGraph(AboutGraphInput {
            handle: "props_text_test".parse()?,
        })
    );

    snapshot!(
        out.text,
        "
# Graph: props_text_test

## Stats

- **Nodes**: 2
- **Edges**: 1 (1 directed)

## Metrics

- **`size`**

## All Available Metric Views

- `size`
- `size~transitive`
- `size~dominated`
- `parents-count`
- `node-count~transitive`
- `node-count~dominated`

"
    );

    Ok(())
}

#[tokio::test]
async fn empty_properties_when_absent() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let out = call_rpc!(
        t,
        AboutGraph(AboutGraphInput {
            handle: handle.parse()?,
        })
    );

    assert!(out.properties.is_empty());

    Ok(())
}
