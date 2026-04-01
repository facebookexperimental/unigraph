// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::AboutGraphInput;
use unigraph_app::ExploreGraphInput;
use unigraph_app::ExploreGraphTarget;
use unigraph_app::NodeMetric;
use unigraph_app::PutConfigsInput;
use unigraph_app::call_rpc;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;

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

node_name
=========
app

"
    );

    Ok(())
}

#[tokio::test]
async fn all_nodes() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let metrics = [
        NodeMetric::Metric {
            name: "size".into(),
        },
        NodeMetric::Metric {
            name: "lines".into(),
        },
        NodeMetric::MetricTransitive {
            name: "size".into(),
        },
        NodeMetric::MetricDominated {
            name: "size".into(),
        },
        NodeMetric::MetricTiered {
            name: "size".into(),
            tier: "eager".into(),
        },
        NodeMetric::MetricTiered {
            name: "size".into(),
            tier: "lazy".into(),
        },
        NodeMetric::ChildrenCount {},
        NodeMetric::CountDominated {},
        NodeMetric::CountTransitive {},
    ];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .all_nodes()
                .metrics(&metrics)
                .sort_by(NodeMetric::MetricTransitive {
                    name: "size".into(),
                })
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

node_name      | children_count | count_dominated | count_transitive | lines | size | size_dominated | size_eager | size_lazy | size_transitive ▼
===============+================+=================+==================+=======+======+================+============+===========+==================
app            |              3 |              12 |               12 |  1200 |  500 |           1985 |       1775 |      1985 |              1985
core           |              3 |               4 |                5 |   600 |  300 |            820 |        780 |       870 |               870
ui             |              3 |               6 |                7 |   800 |  200 |            615 |        545 |       665 |               665
auth           |              2 |               1 |                3 |   420 |  180 |            180 |        480 |       480 |               480
dialogs        |              1 |               1 |                5 |   350 |  120 |            120 |        265 |       385 |               385
db             |              1 |               1 |                2 |   500 |  250 |            250 |        300 |       300 |               300
components     |              3 |               3 |                4 |   400 |  150 |            215 |        265 |       265 |               265
analytics      |              1 |               1 |                2 |   250 |   90 |             90 |         50 |       140 |               140
styles         |              0 |               1 |                1 |   200 |   80 |             80 |         80 |        80 |                80
utils          |              0 |               1 |                1 |   100 |   50 |             50 |         50 |        50 |                50
button_android |              0 |               1 |                1 |    90 |   35 |             35 |         35 |        35 |                35
button_ios     |              0 |               1 |                1 |    80 |   30 |             30 |         30 |        30 |                30

"
    );

    Ok(())
}

#[tokio::test]
async fn drill_into_app() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let metrics = [
        NodeMetric::Metric {
            name: "size".into(),
        },
        NodeMetric::MetricTransitive {
            name: "size".into(),
        },
        NodeMetric::ChildrenCount {},
    ];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&metrics)
                .sort_by(NodeMetric::MetricTransitive {
                    name: "size".into(),
                })
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | children_count | size | size_transitive ▼
==========+================+======+==================
core      |              3 |  300 |               870
ui        |              3 |  200 |               665
utils     |              0 |   50 |                50

"
    );

    Ok(())
}

#[tokio::test]
async fn drill_into_ui_with_tags() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let metrics = [
        NodeMetric::Metric {
            name: "size".into(),
        },
        NodeMetric::ChildrenCount {},
    ];
    let out = call_rpc!(
        t,
        ExploreGraph(Explore::new(gqc_key).node("ui").metrics(&metrics).build())
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: ui

node_name  | children_count | size | tag
===========+================+======+=====
components |              3 |  150 |
styles     |              0 |   80 |
dialogs    |              1 |  120 | lazy

"
    );

    Ok(())
}

#[tokio::test]
async fn drill_into_components_with_dynamic() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let metrics = [NodeMetric::Metric {
        name: "size".into(),
    }];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key.clone())
                .node("components")
                .metrics(&metrics)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: components

node_name      | size | dynamic
===============+======+========================
utils          |   50 |
button_android |   35 | platform:button/android
button_ios     |   30 | platform:button/ios

"
    );

    Ok(())
}

#[tokio::test]
async fn reverse_edges() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let metrics = [NodeMetric::Metric {
        name: "size".into(),
    }];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("utils")
                .metrics(&metrics)
                .structure(GraphStructure::Reverse)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: reverse
Edges of: utils

node_name  | size
===========+=====
analytics  |   90
app        |  500
auth       |  180
components |  150
db         |  250

"
    );

    Ok(())
}

#[tokio::test]
async fn dominator_tree() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let metrics = [
        NodeMetric::Metric {
            name: "size".into(),
        },
        NodeMetric::MetricDominated {
            name: "size".into(),
        },
        NodeMetric::CountDominated {},
    ];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&metrics)
                .sort_by(NodeMetric::MetricDominated {
                    name: "size".into(),
                })
                .structure(GraphStructure::Dominator)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: dominator
Edges of: app

node_name | count_dominated | size | size_dominated ▼
==========+=================+======+=================
core      |               4 |  300 |              820
ui        |               6 |  200 |              615
utils     |               1 |   50 |               50

"
    );

    Ok(())
}

#[tokio::test]
async fn sort_ascending() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let metrics = [NodeMetric::Metric {
        name: "size".into(),
    }];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&metrics)
                .sort_by(NodeMetric::Metric {
                    name: "size".into(),
                })
                .sort_order(SortOrder::Asc)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | size ▲
==========+=======
utils     |     50
ui        |    200
core      |    300

"
    );

    Ok(())
}

#[tokio::test]
async fn offset_and_limit() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;
    let gqc_key = store_gqc(&t, &handle).await?;

    let metrics = [NodeMetric::Metric {
        name: "size".into(),
    }];
    let sort_by = NodeMetric::Metric {
        name: "size".into(),
    };

    // First page: limit 2
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key.clone())
                .node("app")
                .metrics(&metrics)
                .sort_by(sort_by.clone())
                .limit(2)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: app

node_name | size ▼
==========+=======
core      |    300
ui        |    200

(showing 2 of 3 rows, offset 0)
"
    );

    // Second page: offset 2, limit 2
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("app")
                .metrics(&metrics)
                .sort_by(sort_by)
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

node_name | size ▼
==========+=======
utils     |     50

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

    let metrics = [
        NodeMetric::Metric {
            name: "size".into(),
        },
        NodeMetric::MetricTiered {
            name: "size".into(),
            tier: "eager".into(),
        },
        NodeMetric::MetricTiered {
            name: "size".into(),
            tier: "lazy".into(),
        },
    ];
    let out = call_rpc!(
        t,
        ExploreGraph(Explore::new(gqc_key).node("ui").metrics(&metrics).build())
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: ui

node_name  | size | size_eager | size_lazy | tag
===========+======+============+===========+=====
components |  150 |        265 |       265 |
styles     |   80 |         80 |        80 |
dialogs    |  120 |        265 |       385 | lazy

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
    let metrics = [
        NodeMetric::Metric {
            name: "size".into(),
        },
        NodeMetric::Metric {
            name: "lines".into(),
        },
        NodeMetric::MetricTransitive {
            name: "size".into(),
        },
        NodeMetric::MetricDominated {
            name: "size".into(),
        },
        NodeMetric::MetricTiered {
            name: "size".into(),
            tier: "eager".into(),
        },
        NodeMetric::MetricTiered {
            name: "size".into(),
            tier: "lazy".into(),
        },
        NodeMetric::ChildrenCount {},
        NodeMetric::CountDominated {},
        NodeMetric::CountTransitive {},
    ];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(gqc_key)
                .node("components")
                .metrics(&metrics)
                .sort_by(NodeMetric::MetricTransitive {
                    name: "size".into(),
                })
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Edges: forward
Edges of: components

node_name      | count_dominated | count_transitive | lines | size | size_dominated | size_eager | size_lazy | size_transitive ▼ | dynamic
===============+=================+==================+=======+======+================+============+===========+===================+========================
utils          |               1 |                1 |   100 |   50 |             50 |         50 |        50 |                50 |
button_android |               1 |                1 |    90 |   35 |             35 |         35 |        35 |                35 | platform:button/android
button_ios     |               1 |                1 |    80 |   30 |             30 |         30 |        30 |                30 | platform:button/ios

"
    );

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
            handle: handle.clone(),
        })
    );

    assert_eq!(out.stats.num_all_nodes, 12);
    assert!(out.stats.num_all_edges > 0);
    assert_eq!(out.metrics.len(), 2);
    assert!(out.description.is_none());

    snapshot!(
        out.text,
        "
# Graph: explore_test

## Stats

- **Nodes**: 12
- **Edges**: 17 (13 directed, 2 tagged, 2 dynamic)

## Metrics

- `lines`
- `size`

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
            handle: gqc_key.to_string(),
        })
    );

    assert_eq!(out.stats.num_all_nodes, 12);
    assert_eq!(out.metrics.len(), 2);

    // The GQC-resolved graph has the same stats, but the handle in the text
    // is the gqc_key string. We just verify the structured fields above
    // and check that the text starts with the right heading.
    assert!(out.text.starts_with("# Graph: gqc-"));

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────

async fn store_gqc(t: &TestApp, handle: &str) -> Result<GraphQueryConfigKey> {
    let gqc = GraphQueryConfig {
        roots: Default::default(),
        traversal_config: None,
        handle: Some(handle.to_string()),
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

// ── Input builder ───────────────────────────────────────────────

struct Explore<'a> {
    gqc_key: GraphQueryConfigKey,
    target: ExploreGraphTarget,
    metrics: &'a [NodeMetric],
    sort_by: Option<NodeMetric>,
    sort_order: Option<SortOrder>,
    graph_structure: GraphStructure,
    offset: Option<usize>,
    limit: Option<usize>,
}

impl<'a> Explore<'a> {
    fn new(gqc_key: GraphQueryConfigKey) -> Self {
        Self {
            gqc_key,
            target: ExploreGraphTarget::EntryPoints {},
            metrics: &[],
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

    fn metrics(mut self, metrics: &'a [NodeMetric]) -> Self {
        self.metrics = metrics;
        self
    }

    fn sort_by(mut self, sort_by: NodeMetric) -> Self {
        self.sort_by = Some(sort_by);
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
            graph_query_config: None,
            graph_query_config_key: Some(self.gqc_key),
            target: self.target,
            graph_structure: self.graph_structure,
            metrics: self.metrics.to_vec(),
            sort_by: self.sort_by,
            sort_order: self.sort_order,
            offset: self.offset,
            limit: self.limit,
            include_ascii: Some(true),
        }
    }
}
