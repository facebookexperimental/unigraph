// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::AboutGraphInput;
use unigraph_app::ExploreGraphInput;
use unigraph_app::ExploreGraphTarget;
use unigraph_app::GraphHandle;
use unigraph_app::MetricView;
use unigraph_app::PutConfigsInput;
use unigraph_app::call_rpc;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphSettings;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;
use crate::support::fixtures::ingest_two_entry_points_graph;

// ── Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn entry_points() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(t, ExploreGraph(Explore::new(gqc_key).build()));
    snapshot!(
        out.ascii.unwrap(),
        "
Entry points

node_name | lines | lines~transitive | node-count~transitive | parents-count | size#eager | size~transitive |  tier
==========+=======+==================+=======================+===============+============+=================+======
app       | 1,200 |            4,990 |                    12 |             0 |    1.78 kB |         1.99 kB | eager

"
    );

    Ok(())
}

#[tokio::test]
async fn two_entry_points() -> Result<()> {
    let t = init_app();
    // Graph with a standalone `root` node and no explicit entry_points.
    // Entry points are auto-detected as the parentless nodes: `app` + `root`.
    //
    // Because the graph has >1 entry point, the system synthesizes a single
    // `~root~` super-node (see unigraph_core super_root.rs) with edges to both
    // real entry points. The EntryPoints target therefore shows just `~root~`,
    // whose self metrics are 0 but whose transitive metrics aggregate the whole
    // graph (14 nodes = 13 real + the synthetic super-root).
    let handle = ingest_two_entry_points_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(t, ExploreGraph(Explore::new(gqc_key.clone()).build()));
    snapshot!(
        out.ascii.unwrap(),
        "
Entry points

node_name | lines | lines~transitive | node-count~transitive | parents-count | size#eager | size~transitive |  tier
==========+=======+==================+=======================+===============+============+=================+======
~root~    |     0 |            5,690 |                    14 |             0 |    2.18 kB |         2.39 kB | eager

"
    );

    // Drilling into the synthetic super-root reveals the two real entry points.
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("~root~")
                .metrics(&["size~transitive"])
                .sort_by("size~transitive")
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: ~root~

node_name | size~transitive ▼
==========+==================
~root~    |           2.39 kB
----------+------------------
app       |           1.99 kB
root      |           0.45 kB

"
    );

    Ok(())
}

#[tokio::test]
async fn default_metrics_respects_visibility() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    // No .metrics() call → None → visible views only (Hidden views excluded)
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .sort_by("size~transitive")
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | lines | lines~transitive | node-count~transitive | parents-count | size#eager | size~transitive ▼ |  tier
==========+=======+==================+=======================+===============+============+===================+======
app       | 1,200 |            4,990 |                    12 |             0 |    1.78 kB |           1.99 kB | eager
----------+-------+------------------+-----------------------+---------------+------------+-------------------+------
core      |   600 |            1,870 |                     5 |             1 |    0.78 kB |           0.87 kB | eager
ui        |   800 |            2,020 |                     7 |             1 |    0.55 kB |           0.67 kB | eager
utils     |   100 |              100 |                     1 |             5 |    0.05 kB |           0.05 kB | eager

"
    );

    Ok(())
}

#[tokio::test]
async fn metrics_empty_returns_none() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    // .no_metrics() → Some([]) → no metric columns
    let out = call_rpc!(
        t,
        ExploreGraph(Explore::new(gqc_key).node("app").no_metrics().build())
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name
=========
app
---------
core
ui
utils

"
    );

    // Verify arrows have empty metrics maps
    for arrow in &out.arrows {
        assert!(
            arrow.metrics.is_empty(),
            "expected no metrics on arrow '{}', got {:?}",
            arrow.name,
            arrow.metrics
        );
    }

    Ok(())
}

#[tokio::test]
async fn all_nodes() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .all_nodes()
                .metrics(&[
                    "lines",
                    "size~transitive",
                    "size~dominated",
                    "size#eager",
                    "size#lazy",
                    "node-count~dominated",
                    "node-count~transitive",
                ])
                .sort_by("size~transitive")
                .build()
        )
    );

    // Flat list: no parent row, no indent, all 12 nodes, all columns
    assert!(out.node.is_none(), "AllNodes should have no parent row");
    assert_eq!(out.total_arrows_count, 12);
    snapshot!(
        out.ascii.unwrap(),
        "
All reachable nodes

node_name      | lines | node-count~dominated | node-count~transitive | size#eager | size#lazy | size~dominated | size~transitive ▼
===============+=======+======================+=======================+============+===========+================+==================
app            | 1,200 |                   12 |                    12 |    1.78 kB |   1.99 kB |        1.99 kB |           1.99 kB
core           |   600 |                    4 |                     5 |    0.78 kB |   0.87 kB |        0.82 kB |           0.87 kB
ui             |   800 |                    6 |                     7 |    0.55 kB |   0.67 kB |        0.62 kB |           0.67 kB
auth           |   420 |                    1 |                     3 |    0.48 kB |   0.48 kB |        0.18 kB |           0.48 kB
dialogs        |   350 |                    1 |                     5 |    0.26 kB |   0.39 kB |        0.12 kB |           0.39 kB
db             |   500 |                    1 |                     2 |    0.30 kB |   0.30 kB |        0.25 kB |           0.30 kB
components     |   400 |                    3 |                     4 |    0.26 kB |   0.26 kB |        0.22 kB |           0.26 kB
analytics      |   250 |                    1 |                     2 |    0.05 kB |   0.14 kB |        0.09 kB |           0.14 kB
styles         |   200 |                    1 |                     1 |    0.08 kB |   0.08 kB |        0.08 kB |           0.08 kB
utils          |   100 |                    1 |                     1 |    0.05 kB |   0.05 kB |        0.05 kB |           0.05 kB
button_android |    90 |                    1 |                     1 |    0.04 kB |   0.04 kB |        0.04 kB |           0.04 kB
button_ios     |    80 |                    1 |                     1 |    0.03 kB |   0.03 kB |        0.03 kB |           0.03 kB

"
    );

    Ok(())
}

#[tokio::test]
async fn drill_into_app() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&["size~transitive"])
                .sort_by("size~transitive")
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | size~transitive ▼
==========+==================
app       |           1.99 kB
----------+------------------
core      |           0.87 kB
ui        |           0.67 kB
utils     |           0.05 kB

"
    );

    Ok(())
}

#[tokio::test]
async fn drill_into_ui_with_tags() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("ui")
                .metrics(&["size~transitive"])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: ui

node_name  | size~transitive | tag
===========+=================+=====
ui         |         0.67 kB |
-----------+-----------------+-----
components |         0.26 kB |
styles     |         0.08 kB |
dialogs    |         0.39 kB | lazy

"
    );

    Ok(())
}

#[tokio::test]
async fn drill_into_components_with_dynamic() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key.clone())
                .node("components")
                .metrics(&["size~transitive"])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: components

node_name      | size~transitive | dynamic
===============+=================+========================
components     |         0.26 kB |
---------------+-----------------+------------------------
utils          |         0.05 kB |
button_android |         0.04 kB | platform:button/android
button_ios     |         0.03 kB | platform:button/ios

"
    );

    Ok(())
}

#[tokio::test]
async fn reverse_edges() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("utils")
                .metrics(&["size~transitive"])
                .structure(GraphStructure::Reverse)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: reverse
Edges of: utils

node_name  | size~transitive
===========+================
utils      |         0.05 kB
-----------+----------------
analytics  |         0.14 kB
app        |         1.99 kB
auth       |         0.48 kB
components |         0.26 kB
db         |         0.30 kB

"
    );

    Ok(())
}

#[tokio::test]
async fn dominator_tree() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&["size~transitive", "size~dominated", "node-count~dominated"])
                .sort_by("size~dominated")
                .structure(GraphStructure::Dominator)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: dominator
Edges of: app

node_name | node-count~dominated | size~dominated ▼ | size~transitive
==========+======================+==================+================
app       |                   12 |          1.99 kB |         1.99 kB
----------+----------------------+------------------+----------------
core      |                    4 |          0.82 kB |         0.87 kB
ui        |                    6 |          0.62 kB |         0.67 kB
utils     |                    1 |          0.05 kB |         0.05 kB

"
    );

    Ok(())
}

#[tokio::test]
async fn sort_ascending() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&["size~transitive"])
                .sort_by("size~transitive")
                .sort_order(SortOrder::Asc)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | size~transitive ▲
==========+==================
app       |           1.99 kB
----------+------------------
utils     |           0.05 kB
ui        |           0.67 kB
core      |           0.87 kB

"
    );

    Ok(())
}

#[tokio::test]
async fn offset_and_limit() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    // First page: limit 2
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key.clone())
                .node("app")
                .metrics(&["size~transitive"])
                .sort_by("size~transitive")
                .limit(2)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | size~transitive ▼
==========+==================
app       |           1.99 kB
----------+------------------
core      |           0.87 kB
ui        |           0.67 kB

(showing 2 of 3 rows, offset 0)
"
    );

    // Second page: offset 2, limit 2
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&["size~transitive"])
                .sort_by("size~transitive")
                .offset(2)
                .limit(2)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | size~transitive ▼
==========+==================
app       |           1.99 kB
----------+------------------
utils     |           0.05 kB

(showing 1 of 3 rows, offset 2)
"
    );

    Ok(())
}

#[tokio::test]
async fn tiered_metrics() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("ui")
                .metrics(&["size~transitive", "size#eager", "size#lazy"])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: ui

node_name  | size#eager | size#lazy | size~transitive | tag
===========+============+===========+=================+=====
ui         |    0.55 kB |   0.67 kB |         0.67 kB |
-----------+------------+-----------+-----------------+-----
components |    0.26 kB |   0.26 kB |         0.26 kB |
styles     |    0.08 kB |   0.08 kB |         0.08 kB |
dialogs    |    0.26 kB |   0.39 kB |         0.39 kB | lazy

"
    );

    Ok(())
}

#[tokio::test]
async fn exhaustive_columns() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    // All metric types on a node with dynamic edges
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("components")
                .metrics(&[
                    "lines",
                    "size~transitive",
                    "size~dominated",
                    "size#eager",
                    "size#lazy",
                    "node-count~dominated",
                    "node-count~transitive",
                ])
                .sort_by("size~transitive")
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: components

node_name      | lines | node-count~dominated | node-count~transitive | size#eager | size#lazy | size~dominated | size~transitive ▼ | dynamic
===============+=======+======================+=======================+============+===========+================+===================+========================
components     |   400 |                    3 |                     4 |    0.26 kB |   0.26 kB |        0.22 kB |           0.26 kB |
---------------+-------+----------------------+-----------------------+------------+-----------+----------------+-------------------+------------------------
utils          |   100 |                    1 |                     1 |    0.05 kB |   0.05 kB |        0.05 kB |           0.05 kB |
button_android |    90 |                    1 |                     1 |    0.04 kB |   0.04 kB |        0.04 kB |           0.04 kB | platform:button/android
button_ios     |    80 |                    1 |                     1 |    0.03 kB |   0.03 kB |        0.03 kB |           0.03 kB | platform:button/ios

"
    );

    Ok(())
}

// ── Visibility Tests ─────────────────────────────────────────

#[tokio::test]
async fn default_visibility_hides_tiered_but_override_shows_one() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    // Fixture config:
    //   default_visibility: { tiered: Hidden, tiered_dominated: Hidden }
    //   metrics_visibility: { "size#eager": Enabled }
    //
    // So default explore should NOT show lines#eager, lines#lazy, etc.
    // (hidden by default_visibility), but SHOULD show size#eager
    // (per-view override beats the default).
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key.clone())
                .node("app")
                .sort_by("size~transitive")
                .build()
        )
    );

    let ascii = out.ascii.as_ref().unwrap();
    let header_line = ascii
        .lines()
        .find(|l| l.contains("node_name"))
        .expect("should have header line");
    let columns: Vec<&str> = header_line.split(" | ").map(|s| s.trim()).collect();
    assert!(
        columns.contains(&"size#eager"),
        "size#eager should be visible (per-view override), got: {columns:?}"
    );
    assert!(
        !columns.contains(&"lines#eager"),
        "lines#eager should be hidden (default_visibility)"
    );
    assert!(
        !columns.contains(&"size#lazy"),
        "size#lazy should be hidden (default_visibility)"
    );
    assert!(
        !columns.contains(&"lines~dominated"),
        "dominated should be hidden (forward mode)"
    );

    Ok(())
}

#[tokio::test]
async fn explicit_metrics_override_all_visibility() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    // size#lazy is Hidden by default_visibility.tiered, but requesting
    // it explicitly overrides that — you get it regardless.
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&["lines", "size#lazy", "size~transitive"])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | lines | size#lazy | size~transitive
==========+=======+===========+================
app       | 1,200 |   1.99 kB |         1.99 kB
----------+-------+-----------+----------------
core      |   600 |   0.87 kB |         0.87 kB
ui        |   800 |   0.67 kB |         0.67 kB
utils     |   100 |   0.05 kB |         0.05 kB

"
    );

    Ok(())
}

#[tokio::test]
async fn unavailable_metrics_excluded_from_about() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let out = call_rpc!(
        t,
        AboutGraph(AboutGraphInput {
            handle: handle.parse()?,
        })
    );

    let view_names = &out.metric_views;

    // size self-view is Unavailable via MetricsConfig
    assert!(
        !view_names.contains(&"size".to_string()),
        "size self-view should be unavailable"
    );

    // lines tiered is Unavailable via per-metric config → not in the list
    assert!(
        !view_names.contains(&"lines#eager".to_string()),
        "lines#eager should be unavailable (per-metric tiered: Unavailable)"
    );

    // size#eager IS available (tiered is available for size, just hidden by default_visibility)
    assert!(
        view_names.contains(&"size#eager".to_string()),
        "size#eager should be available (Hidden != Unavailable)"
    );

    Ok(())
}

#[tokio::test]
async fn default_visibility_all_hidden() -> Result<()> {
    use unigraph_core::graph_settings::DefaultVisibility;
    use unigraph_core::graph_settings::MetricViewVisibility;
    use unigraph_core::graph_settings::MetricsConfig;

    let t = init_app();
    let gqc_key = ingest_with_settings(
        &t,
        "all_hidden",
        GraphSettings {
            description: None,
            metrics_config: Some(MetricsConfig {
                default_availability: None,
                default_visibility: Some(DefaultVisibility {
                    all: Some(MetricViewVisibility::Hidden),
                    self_view: None,
                    transitive: None,
                    dominated: None,
                    tiered: None,
                    tiered_dominated: None,
                }),
                metrics: None,
                parents_count: None,
                count_transitive: None,
                count_dominated: None,
            }),
            metrics_visibility: None,
            ui_settings: None,
        },
    )
    .await?;

    let out = call_rpc!(t, ExploreGraph(Explore::new(gqc_key).node("app").build()));
    let columns = header_columns(out.ascii.as_ref().unwrap());

    assert_eq!(
        columns,
        vec!["node_name"],
        "all metrics hidden, only node_name column"
    );

    Ok(())
}

#[tokio::test]
async fn default_visibility_all_hidden_with_type_override() -> Result<()> {
    use unigraph_core::graph_settings::DefaultVisibility;
    use unigraph_core::graph_settings::MetricViewVisibility;
    use unigraph_core::graph_settings::MetricsConfig;

    let t = init_app();
    let gqc_key = ingest_with_settings(
        &t,
        "all_hidden_transitive_shown",
        GraphSettings {
            description: None,
            metrics_config: Some(MetricsConfig {
                default_availability: None,
                default_visibility: Some(DefaultVisibility {
                    all: Some(MetricViewVisibility::Hidden),
                    transitive: Some(MetricViewVisibility::Enabled),
                    self_view: None,
                    dominated: None,
                    tiered: None,
                    tiered_dominated: None,
                }),
                metrics: None,
                parents_count: None,
                count_transitive: None,
                count_dominated: None,
            }),
            metrics_visibility: None,
            ui_settings: None,
        },
    )
    .await?;

    let out = call_rpc!(t, ExploreGraph(Explore::new(gqc_key).node("app").build()));
    let columns = header_columns(out.ascii.as_ref().unwrap());

    assert!(
        columns.contains(&"lines~transitive".to_string()),
        "transitive should be visible (type override beats all)"
    );
    assert!(
        columns.contains(&"size~transitive".to_string()),
        "size~transitive too"
    );
    assert!(
        columns.contains(&"node-count~transitive".to_string()),
        "structural transitive too"
    );
    assert!(
        !columns.contains(&"lines".to_string()),
        "self_view should be hidden (all)"
    );
    assert!(
        !columns.contains(&"size".to_string()),
        "size self should be hidden (all)"
    );

    Ok(())
}

#[tokio::test]
async fn default_visibility_all_hidden_with_per_view_override() -> Result<()> {
    use std::collections::BTreeMap;

    use unigraph_core::graph_settings::DefaultVisibility;
    use unigraph_core::graph_settings::MetricViewVisibility;
    use unigraph_core::graph_settings::MetricsConfig;

    let t = init_app();
    let mut overrides = BTreeMap::new();
    overrides.insert("lines".to_string(), MetricViewVisibility::Enabled);

    let gqc_key = ingest_with_settings(
        &t,
        "all_hidden_one_override",
        GraphSettings {
            description: None,
            metrics_config: Some(MetricsConfig {
                default_availability: None,
                default_visibility: Some(DefaultVisibility {
                    all: Some(MetricViewVisibility::Hidden),
                    self_view: None,
                    transitive: None,
                    dominated: None,
                    tiered: None,
                    tiered_dominated: None,
                }),
                metrics: None,
                parents_count: None,
                count_transitive: None,
                count_dominated: None,
            }),
            metrics_visibility: Some(overrides),
            ui_settings: None,
        },
    )
    .await?;

    let out = call_rpc!(t, ExploreGraph(Explore::new(gqc_key).node("app").build()));
    let columns = header_columns(out.ascii.as_ref().unwrap());

    assert!(
        columns.contains(&"lines".to_string()),
        "lines should be visible (per-view override)"
    );
    assert!(
        !columns.contains(&"size~transitive".to_string()),
        "everything else hidden"
    );
    assert_eq!(columns.len(), 2, "only node_name + lines");

    Ok(())
}

// ── AboutGraph Tests ─────────────────────────────────────────

#[tokio::test]
async fn about_graph_by_timeline() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let out = call_rpc!(
        t,
        AboutGraph(AboutGraphInput {
            handle: handle.parse()?,
        })
    );

    assert_eq!(out.stats.num_all_nodes, 12);
    assert!(out.stats.num_all_edges > 0);
    assert!(out.description.is_none());

    // Enabled metric views: 22 total minus 1 Unavailable (size) minus 6 Hidden (lines tiered) = 15
    let view_names = &out.metric_views;
    snapshot!(
        view_names.join("\n"),
        "
lines
lines~transitive
lines~dominated
size~transitive
size~dominated
size#eager
size#eager~dominated
size#lazy
size#lazy~dominated
parents-count
node-count~transitive
node-count~dominated
tier
"
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
- **`size`** — Module size in bytes

## All Available Metric Views

- `lines`
- `lines~transitive`
- `lines~dominated`
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
async fn about_graph_by_gqc_key() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let out = call_rpc!(
        t,
        AboutGraph(AboutGraphInput {
            handle: GraphHandle::GqcKey(gqc_key.clone()),
        })
    );

    assert_eq!(out.stats.num_all_nodes, 12);
    assert!(!out.metric_views.is_empty());

    // The GQC-resolved graph has the same stats, but the handle in the text
    // is the gqc_key string. We just verify the structured fields above
    // and check that the text starts with the right heading.
    assert!(out.text.starts_with("# Graph: gqc_"));

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────

fn parse_metrics(strs: &[&str]) -> Vec<MetricView> {
    strs.iter()
        .map(|s| {
            s.parse()
                .unwrap_or_else(|e| panic!("bad metric view '{s}': {e}"))
        })
        .collect()
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

/// Ingest the explore fixture with custom graph_settings, returning a GQC key.
async fn ingest_with_settings(
    t: &TestApp,
    timeline_id: &str,
    settings: GraphSettings,
) -> Result<GraphQueryConfigKey> {
    let json = include_str!("../support/fixtures/explore_graph.json");
    let mut map_graph = unigraph_core::MapGraph::from_json(json)?;
    map_graph.graph_settings = Some(settings);
    let ag_ser = map_graph.to_array_graph_serializable()?;

    let tid = unigraph_storage_core::TimelineID(timeline_id.to_string());
    t.app
        .db
        .timelines
        .create(
            &tid,
            &unigraph_storage_core::TimelineConfig {
                schema: unigraph_storage_core::TimelineSchema::AdjacentDeltas(
                    unigraph_storage_core::AdjacentDeltasConfig {},
                ),
                external_id_namespace: None,
                blob_storage: Default::default(),
                store_metric_history: None,
            },
            &t.task,
        )
        .await?;

    let key = unigraph_core::GraphTimeKey {
        timeline_id: tid,
        timestamp: unigraph_core::Timestamp::from_unix_timestamp(1000),
        graph_id: unigraph_core::GraphID(0),
    };
    t.app.db.graph.store(&key, &ag_ser, None, &t.task).await?;

    store_gqc(t, timeline_id).await
}

fn header_columns(ascii: &str) -> Vec<String> {
    ascii
        .lines()
        .find(|l| l.contains("node_name"))
        .expect("should have header line")
        .split(" | ")
        .map(|s| s.trim().to_string())
        .collect()
}

// ── Input builder ───────────────────────────────────────────────

struct Explore {
    query: GraphQueryConfig,
    target: ExploreGraphTarget,
    metrics: Option<Vec<MetricView>>,
    sort_by: Option<MetricView>,
    sort_order: Option<SortOrder>,
    graph_structure: GraphStructure,
    offset: Option<usize>,
    limit: Option<usize>,
}

impl Explore {
    fn new(gqc_key: GraphQueryConfigKey) -> Self {
        Self {
            query: GraphQueryConfig {
                handle: GraphHandle::GqcKey(gqc_key),
                roots: None,
                traversal: None,
            },
            target: ExploreGraphTarget::EntryPoints {},
            metrics: None,
            sort_by: None,
            sort_order: None,
            graph_structure: GraphStructure::Forward,
            offset: None,
            limit: None,
        }
    }

    fn node(mut self, name: &str) -> Self {
        self.target = ExploreGraphTarget::Node {
            name: name.to_string(),
        };
        self
    }

    fn all_nodes(mut self) -> Self {
        self.target = ExploreGraphTarget::AllNodes {};
        self
    }

    fn metrics(mut self, strs: &[&str]) -> Self {
        self.metrics = Some(parse_metrics(strs));
        self
    }

    fn no_metrics(mut self) -> Self {
        self.metrics = Some(vec![]);
        self
    }

    fn sort_by(mut self, s: &str) -> Self {
        self.sort_by = Some(
            s.parse()
                .unwrap_or_else(|e| panic!("bad sort_by '{s}': {e}")),
        );
        self
    }

    fn sort_order(mut self, sort_order: SortOrder) -> Self {
        self.sort_order = Some(sort_order);
        self
    }

    fn structure(mut self, graph_structure: GraphStructure) -> Self {
        self.graph_structure = graph_structure;
        self
    }

    fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    fn build(self) -> ExploreGraphInput {
        ExploreGraphInput {
            query: self.query,
            target: self.target,
            graph_structure: self.graph_structure,
            metrics: self.metrics,
            sort_by: self.sort_by,
            sort_order: self.sort_order,
            offset: self.offset,
            limit: self.limit,
            include_ascii: Some(true),
        }
    }
}
