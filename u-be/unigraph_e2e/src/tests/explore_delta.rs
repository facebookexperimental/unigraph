// Copyright (c) Meta Platforms, Inc. and affiliates.

//! ExploreDelta over the `explore_graph` fixture (before) and
//! `explore_graph_after` (after). See `support/fixtures.rs` for exactly what
//! differs between the two.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::ExploreDeltaInput;
use unigraph_app::ExploreGraphTarget;
use unigraph_app::call_rpc;
use unigraph_core::MetricView;
use unigraph_core::config_query::GraphQueryConfig;
use unigraph_core::graph_settings::GraphStructure;
use unigraph_core::graph_settings::SortOrder;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::ingest_delta_semantics;
use crate::support::fixtures::ingest_delta_semantics_after;
use crate::support::fixtures::ingest_explore_graph;
use crate::support::fixtures::ingest_explore_graph_after;

// ── Tests ────────────────────────────────────────────────────────

/// Default columns: the right-hand value plus `∆` for every visible view.
/// `app` itself is unchanged, but its subtree grew.
#[tokio::test]
async fn entry_points() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(t, ExploreDelta(d.build()));
    snapshot!(
        out.ascii.unwrap(),
        "
Delta: entry points

node_name | lines | lines@delta | lines~transitive | lines~transitive@delta | node-count~transitive | node-count~transitive@delta | node_type | parents-count | parents-count@delta | size#eager | size#eager@delta | size~transitive | size~transitive@delta |  tier
==========+=======+=============+==================+========================+=======================+=============================+===========+===============+=====================+============+==================+=================+=======================+======
app       | 1,200 |           0 |            5,135 |                   +145 |                    13 |                          +1 |      root |             0 |                   0 |    2.08 kB |         +0.30 kB |         2.19 kB |              +0.21 kB | eager

"
    );

    Ok(())
}

/// Drilling into a node shows which children changed and how.
#[tokio::test]
async fn drill_into_node() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.node("app")
                .metrics(&["size~transitive", "size~transitive@delta"])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta edges: forward
Delta edges of: app

node_name | change        | size~transitive | size~transitive@delta
==========+===============+=================+======================
app       |               |         2.19 kB |              +0.21 kB
----------+---------------+-----------------+----------------------
core      | EDGES+METRICS |         1.04 kB |              +0.17 kB
ui        | EDGES         |         0.71 kB |              +0.04 kB
utils     |               |         0.05 kB |               0.00 kB

"
    );

    Ok(())
}

/// A node only in the "after" graph is ADDED; one only in "before" is REMOVED.
/// Both show up under `core`, whose edges changed.
#[tokio::test]
async fn added_and_removed() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.node("core")
                .metrics(&["size~transitive", "size~transitive@delta"])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta edges: forward
Delta edges of: core

node_name | change        | edge    | size~transitive | size~transitive@delta | tag
==========+===============+=========+=================+=======================+=====
core      | EDGES+METRICS |         |         1.04 kB |              +0.17 kB |
----------+---------------+---------+-----------------+-----------------------+-----
analytics | REMOVED       | removed |         0.00 kB |              -0.14 kB | lazy
auth      |               |         |         0.48 kB |               0.00 kB |
db        |               |         |         0.30 kB |               0.00 kB |
telemetry | ADDED         | added   |         0.44 kB |              +0.44 kB |

"
    );

    Ok(())
}

/// Edge-level changes are visible even when both endpoints are identical:
/// `ui -> utils` is new, and `ui -> dialogs` was retagged `lazy` -> `deferred`.
/// Neither shows up in `change`, which describes the node — a retag would be
/// recorded on `ui` (the edge's source), not on the `dialogs` row.
#[tokio::test]
async fn edge_added_and_retagged() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(t, ExploreDelta(d.node("ui").no_metrics().build()));
    snapshot!(
        out.ascii.unwrap(),
        "
Delta edges: forward
Delta edges of: ui

node_name  | change | edge     | tag
===========+========+==========+==================
ui         | EDGES  |          |
-----------+--------+----------+------------------
components | EDGES  |          |
dialogs    |        | retagged | lazy ► deferred
styles     |        |          |
utils      |        | added    |

"
    );

    Ok(())
}

/// `@delta` sorts by magnitude by default, so the biggest movers surface
/// regardless of direction.
#[tokio::test]
async fn sort_by_delta_magnitude() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.all_nodes()
                .metrics(&["size~transitive@delta"])
                .sort_by("size~transitive@delta")
                .limit(6)
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta: all reachable nodes

node_name  | change        | size~transitive@delta ▼
===========+===============+========================
telemetry  | ADDED         |                +0.44 kB
app        |               |                +0.21 kB
core       | EDGES+METRICS |                +0.17 kB
analytics  | REMOVED       |                -0.14 kB
button_web | ADDED         |                +0.04 kB
components | EDGES         |                +0.04 kB

(showing 6 of 14 rows, offset 0)
"
    );

    Ok(())
}

/// Left, right, and delta side by side — the arithmetic is visible in one row.
#[tokio::test]
async fn left_right_and_delta_columns() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.node("core")
                .metrics(&[
                    "size~transitive@left",
                    "size~transitive",
                    "size~transitive@delta",
                ])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta edges: forward
Delta edges of: core

node_name | change        | edge    | size~transitive | size~transitive@delta | size~transitive@left | tag
==========+===============+=========+=================+=======================+======================+=====
core      | EDGES+METRICS |         |         1.04 kB |              +0.17 kB |              0.87 kB |
----------+---------------+---------+-----------------+-----------------------+----------------------+-----
analytics | REMOVED       | removed |         0.00 kB |              -0.14 kB |              0.14 kB | lazy
auth      |               |         |         0.48 kB |               0.00 kB |              0.48 kB |
db        |               |         |         0.30 kB |               0.00 kB |              0.30 kB |
telemetry | ADDED         | added   |         0.44 kB |              +0.44 kB |              0.00 kB |

"
    );

    Ok(())
}

/// Tiered `∆` is the *exclusive* number — it ignores nodes that didn't change —
/// while `size~transitive@delta` is a plain `R - L`. `telemetry` is a new node
/// whose dependencies (`db`, `utils`) already existed, so its exclusive delta
/// is just its own size while its plain transitive delta is the whole subtree.
#[tokio::test]
async fn tiered_delta_is_exclusive() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.node("core")
                .metrics(&["size#eager@delta", "size~transitive@delta"])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta edges: forward
Delta edges of: core

node_name | change        | edge    | size#eager@delta | size~transitive@delta | tag
==========+===============+=========+==================+=======================+=====
core      | EDGES+METRICS |         |         +0.26 kB |              +0.17 kB |
----------+---------------+---------+------------------+-----------------------+-----
analytics | REMOVED       | removed |          0.00 kB |              -0.14 kB | lazy
auth      |               |         |          0.00 kB |               0.00 kB |
db        |               |         |          0.00 kB |               0.00 kB |
telemetry | ADDED         | added   |         +0.14 kB |              +0.44 kB |

"
    );

    Ok(())
}

/// The exclusive rule again, for the *other* column that uses it —
/// `node-count~transitive@delta`. The `explore_graph` fixture cannot pin this
/// one: there the added and removed nodes net out to `+1` under either rule,
/// so a regression to plain `R - L` would go unnoticed.
///
/// Here `newcomer` reaches 3 nodes but only *added* itself — `shared_b` and
/// `shared_c` were already in the graph. Exclusive says `+1`; the plain
/// transitive column next to it says the subtree is worth `+35`.
#[tokio::test]
async fn transitive_count_delta_is_exclusive() -> Result<()> {
    let t = init_app();
    let d = Delta::setup_semantics(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.all_nodes()
                .metrics(&[
                    "node-count~transitive@left",
                    "node-count~transitive",
                    "node-count~transitive@delta",
                    "size~transitive@delta",
                ])
                .build()
        )
    );
    snapshot!(out.ascii.unwrap(), "
Delta: all reachable nodes

node_name | change | node-count~transitive | node-count~transitive@delta | node-count~transitive@left | size~transitive@delta
==========+========+=======================+=============================+============================+======================
newcomer  | ADDED  |                     3 |                          +1 |                          0 |                   +35
root      | EDGES  |                     5 |                          +1 |                          4 |                    +5
shared_a  |        |                     3 |                           0 |                          3 |                     0
shared_b  |        |                     2 |                           0 |                          2 |                     0
shared_c  |        |                     1 |                           0 |                          1 |                     0

");

    Ok(())
}

/// Dominated `∆` columns are a plain `R - L`. There is no exclusive variant
/// for them and no UI column either, so this snapshot is the only description
/// of what the RPC hands back.
///
/// `newcomer` gives `shared_b` a second parent, so `shared_b` and `shared_c`
/// leave `shared_a`'s dominated subtree. `shared_a` is otherwise untouched —
/// same size, same edges — yet its dominated deltas are large and negative.
/// The exclusive rule, which skips unchanged nodes, would have reported `0`.
#[tokio::test]
async fn dominated_deltas_are_plain_subtraction() -> Result<()> {
    let t = init_app();
    let d = Delta::setup_semantics(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.all_nodes()
                .structure(GraphStructure::Dominator)
                .metrics(&[
                    "size~dominated@left",
                    "size~dominated",
                    "size~dominated@delta",
                    "node-count~dominated@delta",
                ])
                .build()
        )
    );
    snapshot!(out.ascii.unwrap(), "
Delta: all reachable nodes

node_name | change | node-count~dominated@delta | size~dominated | size~dominated@delta | size~dominated@left
==========+========+============================+================+======================+====================
newcomer  | ADDED  |                         +1 |              5 |                   +5 |                   0
root      | EDGES  |                         +1 |            175 |                   +5 |                 170
shared_a  |        |                         -2 |             40 |                  -30 |                  70
shared_b  |        |                          0 |             30 |                    0 |                  30
shared_c  |        |                          0 |             10 |                    0 |                  10

");

    Ok(())
}

/// Changed-nodes-only drops rows for children that are identical on both
/// sides: `app`'s `utils` edge disappears while `core` and `ui` stay.
#[tokio::test]
async fn changed_nodes_only_drops_unchanged_children() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(d.node("app").changed_only().no_metrics().build())
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta edges: forward
Delta edges of: app
Mode: changed nodes only

node_name | change
==========+==============
app       |
----------+--------------
core      | EDGES+METRICS
ui        | EDGES

"
    );

    Ok(())
}

/// When the nearest changed node isn't a direct neighbour, the row reports how
/// many unchanged nodes were collapsed to reach it. Walking back from `utils`,
/// `core` is only reachable through `db` / `auth` — both unchanged — so it
/// lands at `skipped: 1`.
#[tokio::test]
async fn changed_nodes_only_reports_skips() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.node("utils")
                .structure(GraphStructure::Reverse)
                .changed_only()
                .no_metrics()
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta edges: reverse
Delta edges of: utils
Mode: changed nodes only

node_name  | change        | edge    | skipped
===========+===============+=========+========
utils      |               |         |       0
-----------+---------------+---------+--------
analytics  | REMOVED       | removed |       0
components | EDGES         |         |       0
core       | EDGES+METRICS |         |       1
telemetry  | ADDED         | added   |       0
ui         | EDGES         | added   |       0

"
    );

    Ok(())
}

/// For `AllNodes`, changed-nodes-only is a filter rather than a collapse, so
/// the count of what it hid is reported instead of per-row skips.
#[tokio::test]
async fn changed_nodes_only_hides_unchanged() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(d.all_nodes().changed_only().no_metrics().build())
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta: all reachable nodes
Mode: changed nodes only

node_name  | change
===========+==============
analytics  | REMOVED
button_web | ADDED
components | EDGES
core       | EDGES+METRICS
telemetry  | ADDED
ui         | EDGES

(8 unchanged nodes hidden)
"
    );

    Ok(())
}

#[tokio::test]
async fn pagination() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let page = |offset: usize| {
        d.clone()
            .all_nodes()
            .no_metrics()
            .sort_by("size~transitive@delta")
            .offset(offset)
            .limit(3)
            .build()
    };

    let first = call_rpc!(t, ExploreDelta(page(0)));
    let second = call_rpc!(t, ExploreDelta(page(3)));

    assert_eq!(first.total_arrows_count, 14);
    assert_eq!(second.total_arrows_count, 14);
    snapshot!(
        format!(
            "{}\n=== offset 3 ===\n{}",
            first.ascii.unwrap(),
            second.ascii.unwrap()
        ),
        "
Delta: all reachable nodes

node_name | change
==========+==============
telemetry | ADDED
app       |
core      | EDGES+METRICS

(showing 3 of 14 rows, offset 0)
=== offset 3 ===
Delta: all reachable nodes

node_name  | change
===========+========
analytics  | REMOVED
button_web | ADDED
components | EDGES

(showing 3 of 14 rows, offset 3)
"
    );

    Ok(())
}

/// The merged twin graph is cached per handle pair. The second call also flips
/// on changed-nodes-only, which builds the lazy changed-nodes CSR inside the
/// cached twin — proving the cached instance is reusable, not just readable.
#[tokio::test]
async fn twin_is_cached_across_requests() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let first = call_rpc!(t, ExploreDelta(d.node("core").no_metrics().build()));
    let changed = call_rpc!(
        t,
        ExploreDelta(d.node("core").changed_only().no_metrics().build())
    );
    let again = call_rpc!(t, ExploreDelta(d.node("core").no_metrics().build()));

    assert_eq!(first.ascii, again.ascii, "repeat request must be identical");
    assert_ne!(
        first.ascii, changed.ascii,
        "changed-only must actually change the result"
    );

    Ok(())
}

/// Reverse edges answer "who depends on this?" on both sides at once.
#[tokio::test]
async fn reverse_structure() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let out = call_rpc!(
        t,
        ExploreDelta(
            d.node("db")
                .structure(GraphStructure::Reverse)
                .no_metrics()
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Delta edges: reverse
Delta edges of: db

node_name | change        | edge
==========+===============+======
db        |               |
----------+---------------+------
auth      |               |
core      | EDGES+METRICS |
telemetry | ADDED         | added

"
    );

    Ok(())
}

#[tokio::test]
async fn unknown_metric_is_rejected() -> Result<()> {
    let t = init_app();
    let d = Delta::setup(&t).await?;

    let input = ExploreDeltaInput {
        metrics: Some(vec!["nope~transitive@delta".parse()?]),
        ..d.node("app").build()
    };
    let err = t
        .app
        .exec_rpc(unigraph_app::UnigraphRequest::ExploreDelta(input), &t.task)
        .await;

    // `{:#}` walks the whole chain — ll::Task wraps the real message in a
    // task-name frame.
    let message = match err {
        Ok(unigraph_app::UnigraphResponse::Error(e)) => format!("{:#}", e.into_anyhow()),
        Ok(other) => panic!("expected an error, got a {} response", other.variant_name()),
        Err(e) => format!("{e:#}"),
    };
    assert!(
        message.contains("Unknown metric view(s): nope~transitive@delta"),
        "unexpected error: {message}"
    );

    Ok(())
}

// ── Input builder ───────────────────────────────────────────────

#[derive(Clone)]
struct Delta {
    left: GraphQueryConfig,
    right: GraphQueryConfig,
    target: ExploreGraphTarget,
    graph_structure: GraphStructure,
    changed_nodes_only: bool,
    metrics: Option<Vec<MetricView>>,
    sort_by: Option<MetricView>,
    sort_order: Option<SortOrder>,
    offset: Option<usize>,
    limit: Option<usize>,
}

impl Delta {
    /// Ingest both fixtures and return a builder pointing at them.
    async fn setup(t: &TestApp) -> Result<Self> {
        let before = ingest_explore_graph(t).await?;
        let after = ingest_explore_graph_after(t).await?;
        Self::over(&before, &after)
    }

    /// The minimal chain fixture, for tests about delta *arithmetic*.
    async fn setup_semantics(t: &TestApp) -> Result<Self> {
        let before = ingest_delta_semantics(t).await?;
        let after = ingest_delta_semantics_after(t).await?;
        Self::over(&before, &after)
    }

    fn over(before: &str, after: &str) -> Result<Self> {
        Ok(Self {
            left: bare_gqc(before)?,
            right: bare_gqc(after)?,
            target: ExploreGraphTarget::EntryPoints {},
            graph_structure: GraphStructure::Forward,
            changed_nodes_only: false,
            metrics: None,
            sort_by: None,
            sort_order: None,
            offset: None,
            limit: None,
        })
    }

    fn node(&self, name: &str) -> Self {
        let mut next = self.clone();
        next.target = ExploreGraphTarget::Node {
            name: name.to_string(),
        };
        next
    }

    fn all_nodes(mut self) -> Self {
        self.target = ExploreGraphTarget::AllNodes {};
        self
    }

    fn structure(mut self, graph_structure: GraphStructure) -> Self {
        self.graph_structure = graph_structure;
        self
    }

    fn changed_only(mut self) -> Self {
        self.changed_nodes_only = true;
        self
    }

    fn metrics(mut self, strs: &[&str]) -> Self {
        self.metrics = Some(strs.iter().map(|s| parse_metric(s)).collect());
        self
    }

    fn no_metrics(mut self) -> Self {
        self.metrics = Some(vec![]);
        self
    }

    fn sort_by(mut self, s: &str) -> Self {
        self.sort_by = Some(parse_metric(s));
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

    fn build(&self) -> ExploreDeltaInput {
        let this = self.clone();
        ExploreDeltaInput {
            left: this.left,
            right: this.right,
            target: this.target,
            graph_structure: this.graph_structure,
            changed_nodes_only: this.changed_nodes_only,
            metrics: this.metrics,
            sort_by: this.sort_by,
            sort_order: this.sort_order,
            sort_delta_by_magnitude: None,
            offset: this.offset,
            limit: this.limit,
            include_ascii: Some(true),
        }
    }
}

fn bare_gqc(handle: &str) -> Result<GraphQueryConfig> {
    Ok(GraphQueryConfig {
        handle: handle.parse()?,
        roots: None,
        traversal: None,
    })
}

fn parse_metric(s: &str) -> MetricView {
    s.parse()
        .unwrap_or_else(|e| panic!("bad metric '{s}': {e}"))
}
