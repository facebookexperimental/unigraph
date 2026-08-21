// Copyright (c) Meta Platforms, Inc. and affiliates.

//! End-to-end coverage for the MinCut RPC.
//!
//! Every test runs against the shared `explore_graph` fixture, whose shape is
//! what makes the cases interesting:
//!
//! ```text
//!   app ─┬─► ui ────┬─► components ─┬─► utils
//!        │          │               ├─► button_ios      (dynamic: platform)
//!        │          │               └─► button_android  (dynamic: platform)
//!        │          ├─► styles
//!        │          └─► dialogs ────► components        (tagged: lazy)
//!        ├─► core ──┬─► db ─────────► utils
//!        │          ├─► auth ───────┬─► db
//!        │          │               └─► utils
//!        │          └─► analytics ──► utils             (tagged: lazy)
//!        └─► utils
//! ```
//!
//! `app` is the only entry point, so it is the single source for every cut.

use anyhow::Result;
use k9::snapshot;
use unigraph_app::GraphHandle;
use unigraph_app::MinCutInput;
use unigraph_app::MinCutNamedEdge;
use unigraph_app::PutConfigsInput;
use unigraph_app::call_rpc;
use unigraph_core::config_key::GraphQueryConfigKey;
use unigraph_core::config_query::GraphQueryConfig;

use crate::support::app::TestApp;
use crate::support::app::init_app;
use crate::support::fixtures::ingest_explore_graph;

// ── Tests ────────────────────────────────────────────────────────

/// One table over the whole behaviour space: redundant paths, bottlenecks,
/// protection rerouting the cut, protection blocking it outright, entry-point
/// sinks, and multi-sink features.
#[tokio::test]
async fn min_cut_cases() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest(&t).await?;

    let cases: Vec<Cut> = vec![
        // `db` hangs off `core` via two paths (core->db, auth->db), but both
        // funnel through app->core — so one edge upstream beats two downstream.
        Cut::new(&gqc_key).sinks(&["db"]),
        // Protecting the bottleneck pushes the cut down onto both real parents.
        Cut::new(&gqc_key)
            .sinks(&["db"])
            .protect(&[("app", "core")]),
        // A lazily-tagged single parent: the cut is the one import edge.
        Cut::new(&gqc_key).sinks(&["dialogs"]),
        // Protect it and the cut walks up to the next chokepoint.
        Cut::new(&gqc_key)
            .sinks(&["dialogs"])
            .protect(&[("ui", "dialogs")]),
        // Protect every path and no cut exists at all.
        Cut::new(&gqc_key)
            .sinks(&["dialogs"])
            .protect(&[("ui", "dialogs"), ("app", "ui")]),
        // Multi-node feature: both dynamic platform variants at once. They share
        // `components`, which shares `ui` — one cut covers the whole set.
        Cut::new(&gqc_key).sinks(&["button_ios", "button_android"]),
        // With `app->ui` protected the shared chokepoint is gone, so the minimum
        // cut is the two import edges themselves.
        Cut::new(&gqc_key)
            .sinks(&["button_ios", "button_android"])
            .protect(&[("app", "ui")]),
        // Mixing a cuttable sink with the entry point itself: `app` is reported
        // as uncuttable and the cut covers only `dialogs`.
        Cut::new(&gqc_key).sinks(&["app", "dialogs"]),
        // The entry point alone can never be cut off.
        Cut::new(&gqc_key).sinks(&["app"]),
    ];

    let mut rows = Vec::with_capacity(cases.len());
    for case in cases {
        rows.push(run_case(&t, case).await?);
    }

    snapshot!(rows.join("\n"), "
db => [app->core] (uncuttable=[], blocked=false)
db !{app->core} => [auth->db, core->db] (uncuttable=[], blocked=false)
dialogs => [ui->dialogs] (uncuttable=[], blocked=false)
dialogs !{ui->dialogs} => [app->ui] (uncuttable=[], blocked=false)
dialogs !{ui->dialogs,app->ui} => [] (uncuttable=[], blocked=true)
button_ios,button_android => [app->ui] (uncuttable=[], blocked=false)
button_ios,button_android !{app->ui} => [components->button_android, components->button_ios] (uncuttable=[], blocked=false)
app,dialogs => [ui->dialogs] (uncuttable=[app], blocked=false)
app => [] (uncuttable=[app], blocked=false)
");

    Ok(())
}

/// `utils` is the most-depended-on node in the fixture, which makes it the only
/// sink with a cut big enough to page through.
#[tokio::test]
async fn pagination() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest(&t).await?;

    let full = call_rpc!(t, MinCut(Cut::new(&gqc_key).sinks(&["utils"]).build()));
    let page = call_rpc!(
        t,
        MinCut(
            Cut::new(&gqc_key)
                .sinks(&["utils"])
                .offset(2)
                .limit(2)
                .build()
        )
    );

    // The page must be a window into the same total, not a re-computed cut.
    let report = format!(
        "full: {}\ntotal={} page(offset=2, limit=2): {}\npage total={}",
        format_edges(&full.cut_edges),
        full.total_cut_edges_count,
        format_edges(&page.cut_edges),
        page.total_cut_edges_count,
    );
    snapshot!(
        report,
        "
full: app->core, app->utils, components->utils
total=3 page(offset=2, limit=2): components->utils
page total=3
"
    );

    Ok(())
}

#[tokio::test]
async fn ascii_table() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest(&t).await?;

    let out = call_rpc!(
        t,
        MinCut(
            Cut::new(&gqc_key)
                .sinks(&["button_ios", "button_android"])
                .protect(&[("app", "ui")])
                .build()
        )
    );
    snapshot!(
        out.ascii.unwrap(),
        "
Min cut

Sinks: button_ios, button_android
Protected: app -> ui

from       | to
===========+===============
components | button_android
components | button_ios

2 edge(s) to cut
"
    );

    Ok(())
}

/// The three renderings that aren't a table: nothing to cut, nothing that *can*
/// be cut, and a sink that isn't cuttable at all.
#[tokio::test]
async fn ascii_special_cases() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest(&t).await?;

    let blocked = call_rpc!(
        t,
        MinCut(
            Cut::new(&gqc_key)
                .sinks(&["dialogs"])
                .protect(&[("ui", "dialogs"), ("app", "ui")])
                .build()
        )
    );
    let uncuttable = call_rpc!(t, MinCut(Cut::new(&gqc_key).sinks(&["app"]).build()));
    let paginated = call_rpc!(
        t,
        MinCut(Cut::new(&gqc_key).sinks(&["utils"]).limit(2).build())
    );

    let report = [blocked, uncuttable, paginated]
        .iter()
        .map(|out| out.ascii.clone().unwrap())
        .collect::<Vec<_>>()
        .join("\n\n─────\n\n");
    snapshot!(
        report,
        "
Min cut

Sinks: dialogs
Protected: ui -> dialogs, app -> ui

(no cut possible — the sinks are reachable from the entry points only through protected edges)

─────

Min cut

Sinks: app

warning: entry points cannot be cut off by removing edges, delete them directly: app

─────

Min cut

Sinks: utils

from | to
=====+======
app  | core
app  | utils

3 edge(s) to cut (showing 2, offset 0)
"
    );

    Ok(())
}

#[tokio::test]
async fn rejects_bad_input() -> Result<()> {
    let t = init_app();
    let gqc_key = ingest(&t).await?;

    let cases = vec![
        Cut::new(&gqc_key).sinks(&[]),
        Cut::new(&gqc_key).sinks(&["nope", "also_nope", "db"]),
        Cut::new(&gqc_key)
            .sinks(&["db"])
            .protect(&[("app", "ghost")]),
    ];

    let mut errors = Vec::with_capacity(cases.len());
    for case in cases {
        errors.push(rpc_error(&t, case.build()).await);
    }

    snapshot!(
        errors.join("\n"),
        "
[Task] exec_rpc.MinCut
: at least one sink node is required
[Task] exec_rpc.MinCut
: [Task] min_cut
: sink node(s) not found in graph: nope, also_nope
[Task] exec_rpc.MinCut
: [Task] min_cut
: protected edge(s) reference nodes not in graph: app -> ghost
"
    );

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────

async fn ingest(t: &TestApp) -> Result<GraphQueryConfigKey> {
    let handle = ingest_explore_graph(t).await?;
    store_gqc(t, &handle).await
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

/// One case as a single line:
/// `<sinks> [!{<protected>}] => [<cut>] (uncuttable=[..], blocked=<bool>)`.
async fn run_case(t: &TestApp, case: Cut) -> Result<String> {
    let label = case.label();
    let out = call_rpc!(t, MinCut(case.build()));
    Ok(format!(
        "{label} => [{}] (uncuttable=[{}], blocked={})",
        format_edges(&out.cut_edges),
        out.uncuttable_sinks.join(","),
        out.blocked_by_protected,
    ))
}

fn format_edges(edges: &[MinCutNamedEdge]) -> String {
    edges
        .iter()
        .map(|e| format!("{}->{}", e.from, e.to))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Run an input expected to fail and return the flattened error message.
/// `{:#}` walks the whole chain — `ll::Task` wraps the real message in a
/// task-name frame.
async fn rpc_error(t: &TestApp, input: MinCutInput) -> String {
    let result = t
        .app
        .exec_rpc(unigraph_app::UnigraphRequest::MinCut(input), &t.task)
        .await;
    match result {
        Ok(unigraph_app::UnigraphResponse::Error(e)) => format!("{:#}", e.into_anyhow()),
        Ok(other) => panic!("expected an error, got a {} response", other.variant_name()),
        Err(e) => format!("{e:#}"),
    }
}

// ── Input builder ───────────────────────────────────────────────

struct Cut {
    query: GraphQueryConfig,
    sinks: Vec<String>,
    protected_edges: Vec<MinCutNamedEdge>,
    offset: Option<usize>,
    limit: Option<usize>,
}

impl Cut {
    fn new(gqc_key: &GraphQueryConfigKey) -> Self {
        Self {
            query: GraphQueryConfig {
                handle: GraphHandle::GqcKey(gqc_key.clone()),
                roots: None,
                traversal: None,
            },
            sinks: Vec::new(),
            protected_edges: Vec::new(),
            offset: None,
            limit: None,
        }
    }

    fn sinks(mut self, names: &[&str]) -> Self {
        self.sinks = names.iter().map(|s| (*s).to_string()).collect();
        self
    }

    fn protect(mut self, edges: &[(&str, &str)]) -> Self {
        self.protected_edges = edges
            .iter()
            .map(|(from, to)| MinCutNamedEdge {
                from: (*from).to_string(),
                to: (*to).to_string(),
            })
            .collect();
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

    /// The request side of a table row, so the snapshot shows input and output
    /// together: `<sinks>` or `<sinks> !{<protected>}`.
    fn label(&self) -> String {
        let sinks = self.sinks.join(",");
        if self.protected_edges.is_empty() {
            return sinks;
        }
        let protected = self
            .protected_edges
            .iter()
            .map(|e| format!("{}->{}", e.from, e.to))
            .collect::<Vec<_>>()
            .join(",");
        format!("{sinks} !{{{protected}}}")
    }

    fn build(self) -> MinCutInput {
        MinCutInput {
            query: self.query,
            sinks: self.sinks,
            protected_edges: self.protected_edges,
            offset: self.offset,
            limit: self.limit,
            include_ascii: Some(true),
        }
    }
}
