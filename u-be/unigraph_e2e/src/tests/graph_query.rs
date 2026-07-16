// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::GraphHandle;
use unigraph_app::GraphQueryInput;
use unigraph_app::GraphQueryMapGraphInput;
use unigraph_app::PutConfigsInput;
use unigraph_app::call_rpc;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializablePackage;
use unigraph_core::ArrayGraphSerializablePackageBase64;
use unigraph_core::MapGraph;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_query::GraphQueryConfig;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;

// ── Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn graph_query_returns_packed_graph() -> Result<()> {
    let t = init_app();
    let timeline = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &timeline).await?;

    let out = call_rpc!(
        t,
        GraphQuery(GraphQueryInput {
            query: bare_gqc_from_key(gqc_key),
        })
    );

    // The packed CSR blobs round-trip back into the same graph we stored.
    let unpacked = unpack_to_map_graph(&t, out.package)?;
    snapshot!(
        node_names(&unpacked).join("\n"),
        "
analytics
app
auth
button_android
button_ios
components
core
db
dialogs
styles
ui
utils
"
    );

    Ok(())
}

#[tokio::test]
async fn graph_query_map_graph_returns_map_graph() -> Result<()> {
    let t = init_app();
    let timeline = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &timeline).await?;

    let out = call_rpc!(
        t,
        GraphQueryMapGraph(GraphQueryMapGraphInput {
            query: bare_gqc_from_key(gqc_key),
        })
    );

    // The MapGraph variant returns the graph directly, keyed by node name — no
    // packing/unpacking needed.
    snapshot!(
        node_names(&out.map_graph).join("\n"),
        "
analytics
app
auth
button_android
button_ios
components
core
db
dialogs
styles
ui
utils
"
    );

    Ok(())
}

#[tokio::test]
async fn packed_and_map_graph_variants_agree() -> Result<()> {
    let t = init_app();
    let timeline = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &timeline).await?;

    let packed = call_rpc!(
        t,
        GraphQuery(GraphQueryInput {
            query: bare_gqc_from_key(gqc_key.clone()),
        })
    );
    let mapped = call_rpc!(
        t,
        GraphQueryMapGraph(GraphQueryMapGraphInput {
            query: bare_gqc_from_key(gqc_key),
        })
    );

    // Both RPCs resolve the same query, so the unpacked ArrayGraph and the
    // MapGraph must describe an identical node set, and both echo back the same
    // resolved query config.
    let unpacked = unpack_to_map_graph(&t, packed.package)?;
    assert_eq!(
        node_names(&unpacked),
        node_names(&mapped.map_graph),
        "packed and map-graph variants should contain the same nodes"
    );
    assert_eq!(
        packed.graph_query_config, mapped.graph_query_config,
        "both variants should echo back the same resolved query config"
    );

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────

fn node_names(g: &MapGraph) -> Vec<String> {
    g.nodes.keys().cloned().collect()
}

/// Round-trip a base64 package back into a `MapGraph` for verification.
fn unpack_to_map_graph(
    t: &TestApp,
    package: ArrayGraphSerializablePackageBase64,
) -> Result<MapGraph> {
    let package = ArrayGraphSerializablePackage::from_base64(package)?;
    let ser = ArrayGraphSerializable::unpack(&package, &t.task)?;
    let ag = ser.into_array_graph(&t.task)?;
    ag.to_map_graph()
}

fn bare_gqc_from_key(key: GraphQueryConfigKey) -> GraphQueryConfig {
    GraphQueryConfig {
        handle: GraphHandle::GqcKey(key),
        roots: None,
        traversal: None,
    }
}

async fn store_gqc(t: &TestApp, handle: &str) -> Result<GraphQueryConfigKey> {
    let gqc = GraphQueryConfig {
        handle: handle.parse().unwrap(),
        roots: None,
        traversal: None,
    };
    let put = call_rpc!(
        t,
        PutConfigs(PutConfigsInput {
            traversal_configs: vec![],
            graph_query_configs: vec![gqc],
        })
    );
    Ok(put.graph_query_configs.into_iter().next().unwrap())
}
