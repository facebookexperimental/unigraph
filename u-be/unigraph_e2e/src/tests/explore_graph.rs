// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::ExploreGraphInput;
use unigraph_app::NodeMetric;
use unigraph_app::call_rpc;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;

use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;

// ── Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn entry_points() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let out = call_rpc!(t, ExploreGraph(Explore::new(&handle).build()));
    snapshot!(
        out.ascii.unwrap(),
        "
name
====
app

"
    );

    Ok(())
}

#[tokio::test]
async fn drill_into_app() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

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
            Explore::new(&handle)
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
name    | children_count | size | size_transitive
========+================+======+================
app     |              3 |  500 |            1985
  core  |              3 |  300 |             870
  ui    |              3 |  200 |             665
  utils |              0 |   50 |              50

"
    );

    Ok(())
}

#[tokio::test]
async fn drill_into_ui_with_tags() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let metrics = [
        NodeMetric::Metric {
            name: "size".into(),
        },
        NodeMetric::ChildrenCount {},
    ];
    let out = call_rpc!(
        t,
        ExploreGraph(Explore::new(&handle).node("ui").metrics(&metrics).build())
    );
    snapshot!(
        out.ascii.unwrap(),
        "
name         | children_count | size | tag
=============+================+======+=====
ui           |              3 |  200 |
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

    let metrics = [NodeMetric::Metric {
        name: "size".into(),
    }];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(&handle)
                .node("components")
                .metrics(&metrics)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
name             | size
=================+=====
components       |  150
  utils          |   50
  button_android |   35
  button_ios     |   30

"
    );

    Ok(())
}

#[tokio::test]
async fn reverse_edges() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let metrics = [NodeMetric::Metric {
        name: "size".into(),
    }];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(&handle)
                .node("utils")
                .metrics(&metrics)
                .structure(GraphStructure::Reverse)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
name         | size
=============+=====
utils        |   50
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
            Explore::new(&handle)
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
name    | count_dominated | size | size_dominated
========+=================+======+===============
app     |              12 |  500 |           1985
  core  |               4 |  300 |            820
  ui    |               6 |  200 |            615
  utils |               1 |   50 |             50

"
    );

    Ok(())
}

#[tokio::test]
async fn sort_ascending() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

    let metrics = [NodeMetric::Metric {
        name: "size".into(),
    }];
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(&handle)
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
name    | size
========+=====
app     |  500
  utils |   50
  ui    |  200
  core  |  300

"
    );

    Ok(())
}

#[tokio::test]
async fn offset_and_limit() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

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
            Explore::new(&handle)
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
name   | size
=======+=====
app    |  500
  core |  300
  ui   |  200

(showing 2 of 3 rows, offset 0)
"
    );

    // Second page: offset 2, limit 2
    let out = call_rpc!(
        t,
        ExploreGraph(
            Explore::new(&handle)
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
name    | size
========+=====
app     |  500
  utils |   50

(showing 1 of 3 rows, offset 2)
"
    );

    Ok(())
}

#[tokio::test]
async fn tiered_metrics() -> Result<()> {
    let t = init_app();
    let handle = ingest_explore_graph(&t).await?;

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
        ExploreGraph(Explore::new(&handle).node("ui").metrics(&metrics).build())
    );
    snapshot!(
        out.ascii.unwrap(),
        "
name         | size | size_eager | size_lazy | tag
=============+======+============+===========+=====
ui           |  200 |        545 |       665 |
  components |  150 |        265 |       265 |
  styles     |   80 |         80 |        80 |
  dialogs    |  120 |        265 |       385 | lazy

"
    );

    Ok(())
}

// ── Input builder ───────────────────────────────────────────────

struct Explore<'a> {
    handle: &'a str,
    node: Option<&'a str>,
    metrics: &'a [NodeMetric],
    sort_by: Option<NodeMetric>,
    sort_order: Option<SortOrder>,
    graph_structure: GraphStructure,
    offset: Option<usize>,
    limit: Option<usize>,
}

impl<'a> Explore<'a> {
    fn new(handle: &'a str) -> Self {
        Self {
            handle,
            node: None,
            metrics: &[],
            sort_by: None,
            sort_order: None,
            graph_structure: GraphStructure::Forward,
            offset: None,
            limit: None,
        }
    }

    fn node(mut self, node: &'a str) -> Self {
        self.node = Some(node);
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
            graph_query_config: Some(GraphQueryConfig {
                roots: Default::default(),
                traversal_config: None,
                handle: Some(self.handle.to_string()),
            }),
            graph_query_config_key: None,
            node: self.node.map(|s| s.to_string()),
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
