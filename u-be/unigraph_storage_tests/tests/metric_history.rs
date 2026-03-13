// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use k9::snapshot;
use unigraph_core::ArrayGraphNodes;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializableEdges;
use unigraph_core::ArrayGraphSerializableNodeMetadata;
use unigraph_db::UnigraphDb;
use unigraph_metric_history::NodeMetricSnapshot;
use unigraph_storage_core::*;
use unigraph_storage_sqlite::SqliteStorage;
use unigraph_storage_tests::*;
use unigraph_timestamp::Timestamp;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_db() -> UnigraphDb {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    UnigraphDb::new(sqlite.clone(), sqlite)
}

async fn setup_with_history(db: &UnigraphDb, name: &str, task: &ll::Task) {
    db.timelines
        .create(
            &TimelineID(name.to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: Default::default(),
                store_metric_history: Some(true),
            },
            task,
        )
        .await
        .unwrap();
}

async fn setup_without_history(db: &UnigraphDb, name: &str, task: &ll::Task) {
    db.timelines
        .create(
            &TimelineID(name.to_string()),
            &TimelineConfig {
                schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
                external_id_namespace: None,
                blob_storage: Default::default(),
                store_metric_history: None,
            },
            task,
        )
        .await
        .unwrap();
}

async fn store(
    db: &UnigraphDb,
    timeline: &str,
    graph_id: i64,
    ts: i64,
    scene: &Scene,
    task: &ll::Task,
) {
    let key = GraphTimeKey {
        timeline_id: TimelineID(timeline.to_string()),
        timestamp: Timestamp::from_unix_timestamp(ts),
        graph_id: GraphID(graph_id),
    };
    db.graph.store(&key, &scene.to_graph(), task).await.unwrap();
}

async fn fetch_all_history(
    db: &UnigraphDb,
    timeline: &str,
    nodes: &[&str],
    task: &ll::Task,
) -> BTreeMap<String, Vec<(Timestamp, GraphID, NodeMetricSnapshot)>> {
    let names: Vec<String> = nodes.iter().map(|s| s.to_string()).collect();
    db.metric_history
        .fetch(
            &TimelineID(timeline.to_string()),
            &names,
            Timestamp::from_unix_timestamp(0),
            Timestamp::from_unix_timestamp(2_000_000_000), // ~2033
            task,
        )
        .await
        .unwrap()
}

fn format_history(
    history: &BTreeMap<String, Vec<(Timestamp, GraphID, NodeMetricSnapshot)>>,
) -> String {
    let mut lines = Vec::new();
    for (node, entries) in history {
        lines.push(format!("{node}:"));
        for (ts, gid, snap) in entries {
            let week = unigraph_metric_history::WeekPartition::from_timestamp(*ts);
            let metrics: Vec<String> = snap.iter().map(|(k, v)| format!("{k}={v:.1}")).collect();
            lines.push(format!(
                "  g={:<3} ts={:<10} {:<9} {}",
                gid.0,
                ts.to_unix_timestamp(),
                week.display_key(),
                metrics.join(" ")
            ));
        }
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Scene: a declarative description of what a graph looks like
// ---------------------------------------------------------------------------

struct Scene {
    /// node_name → {metric_name → value}. Missing nodes are absent.
    nodes: BTreeMap<String, BTreeMap<String, f32>>,
}

impl Scene {
    fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    fn node(mut self, name: &str, metrics: &[(&str, f32)]) -> Self {
        let m: BTreeMap<String, f32> = metrics.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        self.nodes.insert(name.to_string(), m);
        self
    }

    fn to_graph(&self) -> ArrayGraphSerializable {
        let names: Vec<&String> = self.nodes.keys().collect();
        let n = names.len();

        let mut buf = String::new();
        let mut offsets = vec![0usize];
        for name in &names {
            buf.push_str(name);
            offsets.push(buf.len());
        }

        let mut all_metrics: BTreeMap<String, Vec<f32>> = BTreeMap::new();
        for node_metrics in self.nodes.values() {
            for k in node_metrics.keys() {
                all_metrics.entry(k.clone()).or_insert_with(|| vec![0.0; n]);
            }
        }
        for (i, name) in names.iter().enumerate() {
            if let Some(node_metrics) = self.nodes.get(*name) {
                for (k, v) in node_metrics {
                    all_metrics.get_mut(k).unwrap()[i] = *v;
                }
            }
        }

        ArrayGraphSerializable {
            node_names_ordered: Arc::new(ArrayGraphNodes::from_parts(buf, offsets)),
            edges: ArrayGraphSerializableEdges {
                directed: vec![],
                directed_offsets: vec![0; n + 1],
                tagged: BTreeMap::new(),
                dynamic: BTreeMap::new(),
            },
            node_metadata: ArrayGraphSerializableNodeMetadata {
                metrics: all_metrics,
                labels: BTreeMap::new(),
                properties: BTreeMap::new(),
            },
            graph_settings: None,
            traversal_config: None,
            budget_configs: BTreeMap::new(),
            entry_points: None,
        }
    }
}

// ---------------------------------------------------------------------------
// XorShift64 PRNG
// ---------------------------------------------------------------------------

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

// ---------------------------------------------------------------------------
// Simple tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn store_and_fetch_basic() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_with_history(&db, "t", &task).await;

    let s = Scene::new()
        .node("app", &[("size", 100.0)])
        .node("lib", &[("size", 50.0)]);

    store(&db, "t", 1, 1000, &s, &task).await;

    let h = fetch_all_history(&db, "t", &["app", "lib"], &task).await;
    snapshot!(
        format_history(&h),
        "
app:
  g=1   ts=1000       1970-W01  size=100.0
lib:
  g=1   ts=1000       1970-W01  size=50.0
"
    );
    Ok(())
}

#[tokio::test]
async fn history_disabled() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_without_history(&db, "t", &task).await;

    let graph = TestGraphTimeline::get_nth(0);
    let key = make_graph_time_key("t", 0, 1000);
    db.graph.store(&key, &graph, &task).await?;
    assert_graphs_equal(&graph, &db.graph.fetch(&key.graph_key(), &task).await?);

    let h = fetch_all_history(&db, "t", &["anything"], &task).await;
    assert!(h.is_empty());
    Ok(())
}

#[tokio::test]
async fn graph_storage_unaffected_by_history() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_with_history(&db, "t", &task).await;

    let graph = TestGraphTimeline::get_nth(99);
    let key = make_graph_time_key("t", 99, 5000);
    db.graph.store(&key, &graph, &task).await?;
    assert_graphs_equal(&graph, &db.graph.fetch(&key.graph_key(), &task).await?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Thorough randomized test
// ---------------------------------------------------------------------------
//
// 200 graphs, 5 nodes, 2 metrics, stored chronologically (AdjacentDeltas
// schema requires monotonic graph_id). Content is randomized via XorShift.
//
// Properties exercised:
//   - Sparse history (metrics change ~30% of the time)
//   - Disappearing nodes ("epsilon" has 40% presence, others 70%)
//   - Multiple ISO weeks (spans ~25 days = 4 weeks)
//   - Cross-week boundary merging in the read path
//   - Value correctness after round-trip
//
// Note: FlatHistory::insert() handles middle insertion at the data structure
// level (see flat_history unit tests). The integration test stores in order
// because the AdjacentDeltas schema enforces monotonic append.

const NODES: &[&str] = &["alpha", "beta", "gamma", "delta", "epsilon"];
const METRICS: &[&str] = &["size", "count"];

fn generate_scenes(n: usize, seed: u64) -> Vec<(i64, i64, Scene)> {
    let mut rng = Rng::new(seed);
    let mut scenes = Vec::with_capacity(n);

    // Seed initial metric values per node.
    let mut current: BTreeMap<&str, BTreeMap<&str, f32>> = BTreeMap::new();
    for &node in NODES {
        let mut m = BTreeMap::new();
        for &metric in METRICS {
            m.insert(metric, (10 + rng.next() % 90) as f32);
        }
        current.insert(node, m);
    }

    // Base: 2025-01-06 00:00 UTC (Monday, ISO W02).
    let base_ts: i64 = 1736121600;

    for i in 0..n {
        let ts = base_ts + (i as i64) * 3600 * 3; // 3-hour spacing
        let graph_id = i as i64;
        let mut scene = Scene::new();

        for &node in NODES {
            let threshold = if node == "epsilon" { 40u64 } else { 70 };
            if (rng.next() % 100) >= threshold {
                continue; // absent
            }

            let base = current.get(node).cloned().unwrap_or_default();
            let mut metrics = BTreeMap::new();
            for &metric in METRICS {
                let prev = base.get(metric).copied().unwrap_or(0.0);
                let val = if (rng.next() % 100) < 30 {
                    (prev + (rng.next() % 10) as f32 - 4.0).max(1.0)
                } else {
                    prev
                };
                metrics.insert(metric, val);
            }
            current.insert(node, metrics.clone());

            let m: Vec<(&str, f32)> = metrics.iter().map(|(&k, &v)| (k, v)).collect();
            scene = scene.node(node, &m);
        }

        scenes.push((graph_id, ts, scene));
    }

    scenes
}

#[tokio::test]
async fn randomized_out_of_order_with_disappearing_nodes() -> Result<()> {
    let db = make_db();
    let task = ll::Task::create_new("test");
    setup_with_history(&db, "chaos", &task).await;

    let scenes = generate_scenes(200, 42);

    // Store in chronological order (AdjacentDeltas requires monotonic graph_id).
    for (gid, ts, scene) in &scenes {
        store(&db, "chaos", *gid, *ts, scene, &task).await;
    }

    // Fetch full history.
    let h = fetch_all_history(&db, "chaos", NODES, &task).await;

    // -- Structural assertions --

    let total_entries: usize = h.values().map(|v| v.len()).sum();
    let max_possible = scenes.len() * NODES.len();

    // Sparse: fewer entries than graphs × nodes.
    assert!(
        total_entries < max_possible,
        "history ({total_entries}) should be sparser than {max_possible}",
    );

    // All 5 nodes appear at some point.
    for &node in NODES {
        assert!(h.contains_key(node), "missing history for {node}");
    }

    // Epsilon (40% presence) has fewer entries than alpha (70%).
    let eps = h.get("epsilon").map_or(0, |v| v.len());
    let alp = h.get("alpha").map_or(0, |v| v.len());
    assert!(
        eps < alp,
        "epsilon ({eps}) should be sparser than alpha ({alp})"
    );

    // History spans at least 3 ISO weeks.
    let weeks: std::collections::BTreeSet<String> = h
        .values()
        .flatten()
        .map(|(ts, _, _)| unigraph_metric_history::WeekPartition::from_timestamp(*ts).display_key())
        .collect();
    assert!(
        weeks.len() >= 3,
        "should span ≥3 weeks, got {}",
        weeks.len()
    );

    // Entries are sorted by (timestamp, graph_id) within each node.
    for (node, entries) in &h {
        for pair in entries.windows(2) {
            let (ts_a, gid_a, _) = &pair[0];
            let (ts_b, gid_b, _) = &pair[1];
            assert!(
                (ts_a, gid_a) < (ts_b, gid_b),
                "{node}: entries not sorted at g={} vs g={}",
                gid_a.0,
                gid_b.0,
            );
        }
    }

    // -- Value correctness: verify metrics match the source scenes --
    //
    // For each history entry, find the corresponding scene and compare.
    let scene_by_gid: BTreeMap<i64, &Scene> = scenes.iter().map(|(gid, _, s)| (*gid, s)).collect();

    for (node, entries) in &h {
        for (_, gid, snapshot) in entries {
            let scene = scene_by_gid
                .get(&gid.0)
                .unwrap_or_else(|| panic!("no scene for g={}", gid.0));
            let expected = scene.nodes.get(node);

            match expected {
                Some(expected_metrics) => {
                    for (metric, &expected_val) in expected_metrics {
                        let actual = snapshot.get(metric).copied().unwrap_or(0.0);
                        assert!(
                            (expected_val as f64 - actual).abs() < 0.01,
                            "{node} g={}: {metric} expected={expected_val}, got={actual}",
                            gid.0,
                        );
                    }
                }
                None => {
                    panic!(
                        "{node} g={}: history entry exists but node is absent in scene",
                        gid.0
                    );
                }
            }
        }
    }

    // Snapshot the full history for human review.
    // (Update with `cargo test -p unigraph_storage_tests --test metric_history -- --ignored -u`
    //  if the random seed or generation logic changes.)
    snapshot!(
        format_history(&h),
        "
alpha:
  g=0   ts=1736121600 2025-W02  count=11.0 size=44.0
  g=1   ts=1736132400 2025-W02  count=8.0 size=44.0
  g=5   ts=1736175600 2025-W02  count=8.0 size=44.0
  g=6   ts=1736186400 2025-W02  count=10.0 size=49.0
  g=8   ts=1736208000 2025-W02  count=10.0 size=54.0
  g=10  ts=1736229600 2025-W02  count=10.0 size=54.0
  g=12  ts=1736251200 2025-W02  count=10.0 size=54.0
  g=13  ts=1736262000 2025-W02  count=12.0 size=54.0
  g=16  ts=1736294400 2025-W02  count=12.0 size=54.0
  g=21  ts=1736348400 2025-W02  count=12.0 size=57.0
  g=22  ts=1736359200 2025-W02  count=15.0 size=59.0
  g=23  ts=1736370000 2025-W02  count=13.0 size=63.0
  g=24  ts=1736380800 2025-W02  count=10.0 size=64.0
  g=25  ts=1736391600 2025-W02  count=10.0 size=66.0
  g=27  ts=1736413200 2025-W02  count=10.0 size=66.0
  g=31  ts=1736456400 2025-W02  count=10.0 size=66.0
  g=33  ts=1736478000 2025-W02  count=10.0 size=66.0
  g=34  ts=1736488800 2025-W02  count=10.0 size=70.0
  g=36  ts=1736510400 2025-W02  count=9.0 size=68.0
  g=37  ts=1736521200 2025-W02  count=9.0 size=69.0
  g=38  ts=1736532000 2025-W02  count=11.0 size=69.0
  g=40  ts=1736553600 2025-W02  count=11.0 size=68.0
  g=41  ts=1736564400 2025-W02  count=16.0 size=68.0
  g=42  ts=1736575200 2025-W02  count=18.0 size=68.0
  g=44  ts=1736596800 2025-W02  count=19.0 size=68.0
  g=46  ts=1736618400 2025-W02  count=21.0 size=68.0
  g=48  ts=1736640000 2025-W02  count=17.0 size=73.0
  g=51  ts=1736672400 2025-W02  count=21.0 size=73.0
  g=55  ts=1736715600 2025-W02  count=21.0 size=76.0
  g=56  ts=1736726400 2025-W03  count=21.0 size=76.0
  g=58  ts=1736748000 2025-W03  count=21.0 size=78.0
  g=62  ts=1736791200 2025-W03  count=26.0 size=78.0
  g=63  ts=1736802000 2025-W03  count=26.0 size=80.0
  g=66  ts=1736834400 2025-W03  count=26.0 size=78.0
  g=67  ts=1736845200 2025-W03  count=23.0 size=83.0
  g=68  ts=1736856000 2025-W03  count=25.0 size=85.0
  g=71  ts=1736888400 2025-W03  count=25.0 size=87.0
  g=73  ts=1736910000 2025-W03  count=25.0 size=87.0
  g=75  ts=1736931600 2025-W03  count=30.0 size=87.0
  g=79  ts=1736974800 2025-W03  count=30.0 size=87.0
  g=81  ts=1736996400 2025-W03  count=30.0 size=87.0
  g=82  ts=1737007200 2025-W03  count=35.0 size=87.0
  g=85  ts=1737039600 2025-W03  count=35.0 size=87.0
  g=90  ts=1737093600 2025-W03  count=35.0 size=87.0
  g=92  ts=1737115200 2025-W03  count=37.0 size=87.0
  g=95  ts=1737147600 2025-W03  count=37.0 size=87.0
  g=97  ts=1737169200 2025-W03  count=42.0 size=87.0
  g=99  ts=1737190800 2025-W03  count=42.0 size=87.0
  g=102 ts=1737223200 2025-W03  count=41.0 size=87.0
  g=103 ts=1737234000 2025-W03  count=46.0 size=87.0
  g=104 ts=1737244800 2025-W03  count=46.0 size=89.0
  g=105 ts=1737255600 2025-W03  count=46.0 size=88.0
  g=108 ts=1737288000 2025-W03  count=46.0 size=84.0
  g=109 ts=1737298800 2025-W03  count=46.0 size=89.0
  g=111 ts=1737320400 2025-W03  count=48.0 size=89.0
  g=112 ts=1737331200 2025-W04  count=48.0 size=87.0
  g=113 ts=1737342000 2025-W04  count=46.0 size=88.0
  g=114 ts=1737352800 2025-W04  count=45.0 size=88.0
  g=116 ts=1737374400 2025-W04  count=45.0 size=84.0
  g=118 ts=1737396000 2025-W04  count=45.0 size=88.0
  g=122 ts=1737439200 2025-W04  count=45.0 size=91.0
  g=123 ts=1737450000 2025-W04  count=47.0 size=91.0
  g=125 ts=1737471600 2025-W04  count=47.0 size=87.0
  g=126 ts=1737482400 2025-W04  count=47.0 size=86.0
  g=128 ts=1737504000 2025-W04  count=47.0 size=86.0
  g=130 ts=1737525600 2025-W04  count=47.0 size=83.0
  g=133 ts=1737558000 2025-W04  count=47.0 size=88.0
  g=134 ts=1737568800 2025-W04  count=46.0 size=88.0
  g=138 ts=1737612000 2025-W04  count=46.0 size=88.0
  g=140 ts=1737633600 2025-W04  count=46.0 size=88.0
  g=142 ts=1737655200 2025-W04  count=44.0 size=93.0
  g=144 ts=1737676800 2025-W04  count=44.0 size=90.0
  g=147 ts=1737709200 2025-W04  count=44.0 size=90.0
  g=152 ts=1737763200 2025-W04  count=44.0 size=90.0
  g=154 ts=1737784800 2025-W04  count=47.0 size=90.0
  g=158 ts=1737828000 2025-W04  count=47.0 size=86.0
  g=160 ts=1737849600 2025-W04  count=47.0 size=87.0
  g=162 ts=1737871200 2025-W04  count=47.0 size=90.0
  g=165 ts=1737903600 2025-W04  count=47.0 size=90.0
  g=168 ts=1737936000 2025-W05  count=47.0 size=90.0
  g=170 ts=1737957600 2025-W05  count=47.0 size=90.0
  g=171 ts=1737968400 2025-W05  count=47.0 size=89.0
  g=176 ts=1738022400 2025-W05  count=44.0 size=86.0
  g=178 ts=1738044000 2025-W05  count=44.0 size=84.0
  g=181 ts=1738076400 2025-W05  count=44.0 size=84.0
  g=183 ts=1738098000 2025-W05  count=44.0 size=82.0
  g=184 ts=1738108800 2025-W05  count=44.0 size=81.0
  g=186 ts=1738130400 2025-W05  count=44.0 size=81.0
  g=187 ts=1738141200 2025-W05  count=44.0 size=80.0
  g=189 ts=1738162800 2025-W05  count=41.0 size=80.0
  g=191 ts=1738184400 2025-W05  count=41.0 size=85.0
  g=194 ts=1738216800 2025-W05  count=41.0 size=85.0
  g=196 ts=1738238400 2025-W05  count=41.0 size=90.0
  g=198 ts=1738260000 2025-W05  count=41.0 size=90.0
beta:
  g=0   ts=1736121600 2025-W02  count=84.0 size=94.0
  g=1   ts=1736132400 2025-W02  count=82.0 size=94.0
  g=3   ts=1736154000 2025-W02  count=82.0 size=90.0
  g=5   ts=1736175600 2025-W02  count=82.0 size=90.0
  g=7   ts=1736197200 2025-W02  count=82.0 size=90.0
  g=11  ts=1736240400 2025-W02  count=84.0 size=90.0
  g=13  ts=1736262000 2025-W02  count=84.0 size=91.0
  g=15  ts=1736283600 2025-W02  count=84.0 size=91.0
  g=18  ts=1736316000 2025-W02  count=87.0 size=92.0
  g=19  ts=1736326800 2025-W02  count=86.0 size=92.0
  g=20  ts=1736337600 2025-W02  count=84.0 size=92.0
  g=21  ts=1736348400 2025-W02  count=87.0 size=92.0
  g=26  ts=1736402400 2025-W02  count=87.0 size=92.0
  g=28  ts=1736424000 2025-W02  count=84.0 size=92.0
  g=32  ts=1736467200 2025-W02  count=89.0 size=92.0
  g=33  ts=1736478000 2025-W02  count=89.0 size=94.0
  g=35  ts=1736499600 2025-W02  count=89.0 size=91.0
  g=36  ts=1736510400 2025-W02  count=89.0 size=92.0
  g=37  ts=1736521200 2025-W02  count=85.0 size=92.0
  g=38  ts=1736532000 2025-W02  count=85.0 size=93.0
  g=39  ts=1736542800 2025-W02  count=86.0 size=92.0
  g=41  ts=1736564400 2025-W02  count=86.0 size=92.0
  g=43  ts=1736586000 2025-W02  count=89.0 size=92.0
  g=46  ts=1736618400 2025-W02  count=89.0 size=92.0
  g=47  ts=1736629200 2025-W02  count=85.0 size=92.0
  g=49  ts=1736650800 2025-W02  count=84.0 size=92.0
  g=51  ts=1736672400 2025-W02  count=84.0 size=92.0
  g=54  ts=1736704800 2025-W02  count=84.0 size=92.0
  g=58  ts=1736748000 2025-W03  count=80.0 size=93.0
  g=60  ts=1736769600 2025-W03  count=80.0 size=93.0
  g=65  ts=1736823600 2025-W03  count=77.0 size=93.0
  g=66  ts=1736834400 2025-W03  count=77.0 size=94.0
  g=70  ts=1736877600 2025-W03  count=77.0 size=93.0
  g=72  ts=1736899200 2025-W03  count=77.0 size=91.0
  g=75  ts=1736931600 2025-W03  count=77.0 size=91.0
  g=76  ts=1736942400 2025-W03  count=76.0 size=93.0
  g=80  ts=1736985600 2025-W03  count=76.0 size=97.0
  g=82  ts=1737007200 2025-W03  count=79.0 size=97.0
  g=83  ts=1737018000 2025-W03  count=79.0 size=98.0
  g=85  ts=1737039600 2025-W03  count=79.0 size=100.0
  g=86  ts=1737050400 2025-W03  count=79.0 size=99.0
  g=87  ts=1737061200 2025-W03  count=76.0 size=99.0
  g=88  ts=1737072000 2025-W03  count=81.0 size=99.0
  g=90  ts=1737093600 2025-W03  count=78.0 size=99.0
  g=91  ts=1737104400 2025-W03  count=74.0 size=99.0
  g=92  ts=1737115200 2025-W03  count=74.0 size=95.0
  g=100 ts=1737201600 2025-W03  count=74.0 size=96.0
  g=104 ts=1737244800 2025-W03  count=74.0 size=96.0
  g=107 ts=1737277200 2025-W03  count=74.0 size=96.0
  g=111 ts=1737320400 2025-W03  count=70.0 size=97.0
  g=112 ts=1737331200 2025-W04  count=70.0 size=97.0
  g=113 ts=1737342000 2025-W04  count=67.0 size=97.0
  g=115 ts=1737363600 2025-W04  count=66.0 size=97.0
  g=116 ts=1737374400 2025-W04  count=63.0 size=97.0
  g=119 ts=1737406800 2025-W04  count=67.0 size=97.0
  g=120 ts=1737417600 2025-W04  count=70.0 size=99.0
  g=121 ts=1737428400 2025-W04  count=70.0 size=97.0
  g=125 ts=1737471600 2025-W04  count=72.0 size=97.0
  g=127 ts=1737493200 2025-W04  count=75.0 size=97.0
  g=128 ts=1737504000 2025-W04  count=75.0 size=94.0
  g=129 ts=1737514800 2025-W04  count=75.0 size=96.0
  g=131 ts=1737536400 2025-W04  count=75.0 size=100.0
  g=133 ts=1737558000 2025-W04  count=75.0 size=100.0
  g=135 ts=1737579600 2025-W04  count=75.0 size=100.0
  g=138 ts=1737612000 2025-W04  count=75.0 size=102.0
  g=141 ts=1737644400 2025-W04  count=75.0 size=99.0
  g=143 ts=1737666000 2025-W04  count=71.0 size=99.0
  g=145 ts=1737687600 2025-W04  count=71.0 size=102.0
  g=146 ts=1737698400 2025-W04  count=74.0 size=102.0
  g=147 ts=1737709200 2025-W04  count=74.0 size=100.0
  g=154 ts=1737784800 2025-W04  count=74.0 size=100.0
  g=157 ts=1737817200 2025-W04  count=77.0 size=99.0
  g=158 ts=1737828000 2025-W04  count=77.0 size=104.0
  g=161 ts=1737860400 2025-W04  count=77.0 size=104.0
  g=163 ts=1737882000 2025-W04  count=77.0 size=109.0
  g=166 ts=1737914400 2025-W04  count=77.0 size=109.0
  g=168 ts=1737936000 2025-W05  count=77.0 size=109.0
  g=169 ts=1737946800 2025-W05  count=74.0 size=109.0
  g=171 ts=1737968400 2025-W05  count=74.0 size=109.0
  g=172 ts=1737979200 2025-W05  count=74.0 size=114.0
  g=173 ts=1737990000 2025-W05  count=73.0 size=114.0
  g=176 ts=1738022400 2025-W05  count=77.0 size=114.0
  g=179 ts=1738054800 2025-W05  count=77.0 size=114.0
  g=180 ts=1738065600 2025-W05  count=75.0 size=114.0
  g=182 ts=1738087200 2025-W05  count=79.0 size=114.0
  g=184 ts=1738108800 2025-W05  count=75.0 size=114.0
  g=185 ts=1738119600 2025-W05  count=72.0 size=114.0
  g=187 ts=1738141200 2025-W05  count=70.0 size=115.0
  g=189 ts=1738162800 2025-W05  count=70.0 size=116.0
  g=191 ts=1738184400 2025-W05  count=67.0 size=116.0
  g=193 ts=1738206000 2025-W05  count=63.0 size=115.0
  g=195 ts=1738227600 2025-W05  count=63.0 size=115.0
  g=196 ts=1738238400 2025-W05  count=60.0 size=115.0
  g=198 ts=1738260000 2025-W05  count=60.0 size=115.0
delta:
  g=0   ts=1736121600 2025-W02  count=94.0 size=65.0
  g=1   ts=1736132400 2025-W02  count=91.0 size=64.0
  g=4   ts=1736164800 2025-W02  count=91.0 size=63.0
  g=7   ts=1736197200 2025-W02  count=95.0 size=63.0
  g=10  ts=1736229600 2025-W02  count=99.0 size=63.0
  g=11  ts=1736240400 2025-W02  count=96.0 size=63.0
  g=13  ts=1736262000 2025-W02  count=100.0 size=63.0
  g=16  ts=1736294400 2025-W02  count=100.0 size=63.0
  g=18  ts=1736316000 2025-W02  count=100.0 size=63.0
  g=23  ts=1736370000 2025-W02  count=100.0 size=63.0
  g=26  ts=1736402400 2025-W02  count=96.0 size=59.0
  g=27  ts=1736413200 2025-W02  count=92.0 size=59.0
  g=29  ts=1736434800 2025-W02  count=92.0 size=59.0
  g=32  ts=1736467200 2025-W02  count=92.0 size=64.0
  g=33  ts=1736478000 2025-W02  count=97.0 size=64.0
  g=40  ts=1736553600 2025-W02  count=96.0 size=64.0
  g=41  ts=1736564400 2025-W02  count=97.0 size=64.0
  g=42  ts=1736575200 2025-W02  count=97.0 size=65.0
  g=44  ts=1736596800 2025-W02  count=97.0 size=65.0
  g=45  ts=1736607600 2025-W02  count=100.0 size=66.0
  g=47  ts=1736629200 2025-W02  count=97.0 size=66.0
  g=48  ts=1736640000 2025-W02  count=94.0 size=64.0
  g=49  ts=1736650800 2025-W02  count=94.0 size=67.0
  g=52  ts=1736683200 2025-W02  count=94.0 size=64.0
  g=54  ts=1736704800 2025-W02  count=97.0 size=64.0
  g=55  ts=1736715600 2025-W02  count=100.0 size=69.0
  g=56  ts=1736726400 2025-W03  count=100.0 size=69.0
  g=58  ts=1736748000 2025-W03  count=98.0 size=71.0
  g=62  ts=1736791200 2025-W03  count=98.0 size=71.0
  g=65  ts=1736823600 2025-W03  count=98.0 size=71.0
  g=66  ts=1736834400 2025-W03  count=98.0 size=76.0
  g=68  ts=1736856000 2025-W03  count=98.0 size=76.0
  g=70  ts=1736877600 2025-W03  count=98.0 size=76.0
  g=75  ts=1736931600 2025-W03  count=96.0 size=76.0
  g=76  ts=1736942400 2025-W03  count=98.0 size=76.0
  g=78  ts=1736964000 2025-W03  count=98.0 size=76.0
  g=84  ts=1737028800 2025-W03  count=98.0 size=76.0
  g=85  ts=1737039600 2025-W03  count=96.0 size=81.0
  g=89  ts=1737082800 2025-W03  count=96.0 size=81.0
  g=93  ts=1737126000 2025-W03  count=96.0 size=82.0
  g=94  ts=1737136800 2025-W03  count=97.0 size=79.0
  g=96  ts=1737158400 2025-W03  count=96.0 size=79.0
  g=99  ts=1737190800 2025-W03  count=97.0 size=79.0
  g=100 ts=1737201600 2025-W03  count=97.0 size=82.0
  g=104 ts=1737244800 2025-W03  count=93.0 size=82.0
  g=105 ts=1737255600 2025-W03  count=93.0 size=86.0
  g=106 ts=1737266400 2025-W03  count=93.0 size=82.0
  g=107 ts=1737277200 2025-W03  count=97.0 size=82.0
  g=110 ts=1737309600 2025-W03  count=97.0 size=82.0
  g=114 ts=1737352800 2025-W04  count=97.0 size=82.0
  g=117 ts=1737385200 2025-W04  count=97.0 size=82.0
  g=118 ts=1737396000 2025-W04  count=97.0 size=84.0
  g=122 ts=1737439200 2025-W04  count=97.0 size=84.0
  g=123 ts=1737450000 2025-W04  count=96.0 size=84.0
  g=126 ts=1737482400 2025-W04  count=96.0 size=82.0
  g=127 ts=1737493200 2025-W04  count=101.0 size=82.0
  g=129 ts=1737514800 2025-W04  count=101.0 size=86.0
  g=131 ts=1737536400 2025-W04  count=101.0 size=86.0
  g=132 ts=1737547200 2025-W04  count=100.0 size=86.0
  g=133 ts=1737558000 2025-W04  count=100.0 size=88.0
  g=135 ts=1737579600 2025-W04  count=100.0 size=86.0
  g=138 ts=1737612000 2025-W04  count=100.0 size=84.0
  g=141 ts=1737644400 2025-W04  count=100.0 size=89.0
  g=142 ts=1737655200 2025-W04  count=98.0 size=94.0
  g=143 ts=1737666000 2025-W04  count=98.0 size=99.0
  g=148 ts=1737720000 2025-W04  count=96.0 size=102.0
  g=156 ts=1737806400 2025-W04  count=95.0 size=104.0
  g=157 ts=1737817200 2025-W04  count=97.0 size=104.0
  g=160 ts=1737849600 2025-W04  count=97.0 size=102.0
  g=161 ts=1737860400 2025-W04  count=95.0 size=102.0
  g=164 ts=1737892800 2025-W04  count=100.0 size=102.0
  g=165 ts=1737903600 2025-W04  count=97.0 size=102.0
  g=169 ts=1737946800 2025-W05  count=97.0 size=102.0
  g=171 ts=1737968400 2025-W05  count=98.0 size=101.0
  g=175 ts=1738011600 2025-W05  count=98.0 size=102.0
  g=176 ts=1738022400 2025-W05  count=98.0 size=98.0
  g=181 ts=1738076400 2025-W05  count=103.0 size=101.0
  g=183 ts=1738098000 2025-W05  count=103.0 size=101.0
  g=184 ts=1738108800 2025-W05  count=104.0 size=101.0
  g=186 ts=1738130400 2025-W05  count=105.0 size=101.0
  g=187 ts=1738141200 2025-W05  count=110.0 size=101.0
  g=189 ts=1738162800 2025-W05  count=110.0 size=105.0
  g=191 ts=1738184400 2025-W05  count=111.0 size=105.0
  g=193 ts=1738206000 2025-W05  count=111.0 size=108.0
  g=194 ts=1738216800 2025-W05  count=111.0 size=109.0
epsilon:
  g=0   ts=1736121600 2025-W02  count=48.0 size=76.0
  g=9   ts=1736218800 2025-W02  count=48.0 size=73.0
  g=13  ts=1736262000 2025-W02  count=48.0 size=73.0
  g=15  ts=1736283600 2025-W02  count=48.0 size=73.0
  g=18  ts=1736316000 2025-W02  count=48.0 size=69.0
  g=23  ts=1736370000 2025-W02  count=51.0 size=69.0
  g=25  ts=1736391600 2025-W02  count=55.0 size=69.0
  g=27  ts=1736413200 2025-W02  count=58.0 size=66.0
  g=30  ts=1736445600 2025-W02  count=58.0 size=66.0
  g=31  ts=1736456400 2025-W02  count=58.0 size=63.0
  g=34  ts=1736488800 2025-W02  count=58.0 size=63.0
  g=36  ts=1736510400 2025-W02  count=58.0 size=63.0
  g=39  ts=1736542800 2025-W02  count=58.0 size=63.0
  g=44  ts=1736596800 2025-W02  count=56.0 size=65.0
  g=49  ts=1736650800 2025-W02  count=58.0 size=70.0
  g=58  ts=1736748000 2025-W03  count=56.0 size=70.0
  g=61  ts=1736780400 2025-W03  count=56.0 size=70.0
  g=63  ts=1736802000 2025-W03  count=56.0 size=70.0
  g=68  ts=1736856000 2025-W03  count=52.0 size=70.0
  g=69  ts=1736866800 2025-W03  count=50.0 size=70.0
  g=70  ts=1736877600 2025-W03  count=52.0 size=66.0
  g=72  ts=1736899200 2025-W03  count=48.0 size=67.0
  g=76  ts=1736942400 2025-W03  count=48.0 size=67.0
  g=82  ts=1737007200 2025-W03  count=48.0 size=66.0
  g=84  ts=1737028800 2025-W03  count=45.0 size=66.0
  g=87  ts=1737061200 2025-W03  count=45.0 size=66.0
  g=91  ts=1737104400 2025-W03  count=46.0 size=66.0
  g=92  ts=1737115200 2025-W03  count=46.0 size=64.0
  g=94  ts=1737136800 2025-W03  count=46.0 size=62.0
  g=98  ts=1737180000 2025-W03  count=46.0 size=62.0
  g=103 ts=1737234000 2025-W03  count=44.0 size=62.0
  g=106 ts=1737266400 2025-W03  count=44.0 size=62.0
  g=112 ts=1737331200 2025-W04  count=44.0 size=59.0
  g=113 ts=1737342000 2025-W04  count=46.0 size=59.0
  g=114 ts=1737352800 2025-W04  count=42.0 size=59.0
  g=116 ts=1737374400 2025-W04  count=38.0 size=59.0
  g=119 ts=1737406800 2025-W04  count=38.0 size=56.0
  g=121 ts=1737428400 2025-W04  count=38.0 size=53.0
  g=123 ts=1737450000 2025-W04  count=38.0 size=58.0
  g=127 ts=1737493200 2025-W04  count=38.0 size=54.0
  g=130 ts=1737525600 2025-W04  count=38.0 size=58.0
  g=132 ts=1737547200 2025-W04  count=38.0 size=58.0
  g=133 ts=1737558000 2025-W04  count=36.0 size=58.0
  g=135 ts=1737579600 2025-W04  count=36.0 size=58.0
  g=137 ts=1737601200 2025-W04  count=38.0 size=58.0
  g=141 ts=1737644400 2025-W04  count=36.0 size=60.0
  g=143 ts=1737666000 2025-W04  count=36.0 size=60.0
  g=145 ts=1737687600 2025-W04  count=36.0 size=60.0
  g=152 ts=1737763200 2025-W04  count=38.0 size=60.0
  g=155 ts=1737795600 2025-W04  count=34.0 size=60.0
  g=158 ts=1737828000 2025-W04  count=34.0 size=60.0
  g=159 ts=1737838800 2025-W04  count=34.0 size=56.0
  g=160 ts=1737849600 2025-W04  count=34.0 size=55.0
  g=163 ts=1737882000 2025-W04  count=34.0 size=55.0
  g=167 ts=1737925200 2025-W04  count=30.0 size=55.0
  g=168 ts=1737936000 2025-W05  count=30.0 size=55.0
  g=172 ts=1737979200 2025-W05  count=30.0 size=55.0
  g=177 ts=1738033200 2025-W05  count=28.0 size=55.0
  g=183 ts=1738098000 2025-W05  count=28.0 size=55.0
  g=185 ts=1738119600 2025-W05  count=28.0 size=52.0
  g=187 ts=1738141200 2025-W05  count=28.0 size=52.0
  g=190 ts=1738173600 2025-W05  count=28.0 size=52.0
  g=192 ts=1738195200 2025-W05  count=28.0 size=48.0
  g=194 ts=1738216800 2025-W05  count=28.0 size=51.0
  g=198 ts=1738260000 2025-W05  count=28.0 size=50.0
  g=199 ts=1738270800 2025-W05  count=31.0 size=50.0
gamma:
  g=0   ts=1736121600 2025-W02  count=67.0 size=82.0
  g=1   ts=1736132400 2025-W02  count=70.0 size=82.0
  g=7   ts=1736197200 2025-W02  count=70.0 size=82.0
  g=8   ts=1736208000 2025-W02  count=72.0 size=82.0
  g=9   ts=1736218800 2025-W02  count=72.0 size=87.0
  g=11  ts=1736240400 2025-W02  count=72.0 size=87.0
  g=12  ts=1736251200 2025-W02  count=68.0 size=92.0
  g=17  ts=1736305200 2025-W02  count=68.0 size=89.0
  g=20  ts=1736337600 2025-W02  count=68.0 size=85.0
  g=21  ts=1736348400 2025-W02  count=64.0 size=85.0
  g=22  ts=1736359200 2025-W02  count=64.0 size=88.0
  g=24  ts=1736380800 2025-W02  count=64.0 size=88.0
  g=25  ts=1736391600 2025-W02  count=67.0 size=88.0
  g=27  ts=1736413200 2025-W02  count=65.0 size=85.0
  g=30  ts=1736445600 2025-W02  count=65.0 size=85.0
  g=34  ts=1736488800 2025-W02  count=65.0 size=85.0
  g=37  ts=1736521200 2025-W02  count=65.0 size=86.0
  g=39  ts=1736542800 2025-W02  count=63.0 size=86.0
  g=43  ts=1736586000 2025-W02  count=63.0 size=86.0
  g=47  ts=1736629200 2025-W02  count=63.0 size=91.0
  g=49  ts=1736650800 2025-W02  count=66.0 size=91.0
  g=50  ts=1736661600 2025-W02  count=64.0 size=91.0
  g=51  ts=1736672400 2025-W02  count=63.0 size=96.0
  g=54  ts=1736704800 2025-W02  count=67.0 size=96.0
  g=56  ts=1736726400 2025-W03  count=67.0 size=101.0
  g=58  ts=1736748000 2025-W03  count=69.0 size=101.0
  g=60  ts=1736769600 2025-W03  count=69.0 size=102.0
  g=65  ts=1736823600 2025-W03  count=74.0 size=102.0
  g=68  ts=1736856000 2025-W03  count=74.0 size=102.0
  g=71  ts=1736888400 2025-W03  count=74.0 size=102.0
  g=75  ts=1736931600 2025-W03  count=71.0 size=105.0
  g=77  ts=1736953200 2025-W03  count=71.0 size=105.0
  g=78  ts=1736964000 2025-W03  count=73.0 size=105.0
  g=81  ts=1736996400 2025-W03  count=71.0 size=105.0
  g=83  ts=1737018000 2025-W03  count=71.0 size=105.0
  g=85  ts=1737039600 2025-W03  count=71.0 size=101.0
  g=86  ts=1737050400 2025-W03  count=72.0 size=101.0
  g=88  ts=1737072000 2025-W03  count=72.0 size=101.0
  g=90  ts=1737093600 2025-W03  count=72.0 size=99.0
  g=93  ts=1737126000 2025-W03  count=70.0 size=99.0
  g=97  ts=1737169200 2025-W03  count=70.0 size=100.0
  g=99  ts=1737190800 2025-W03  count=75.0 size=100.0
  g=100 ts=1737201600 2025-W03  count=76.0 size=96.0
  g=102 ts=1737223200 2025-W03  count=76.0 size=96.0
  g=103 ts=1737234000 2025-W03  count=76.0 size=92.0
  g=105 ts=1737255600 2025-W03  count=76.0 size=92.0
  g=106 ts=1737266400 2025-W03  count=76.0 size=94.0
  g=109 ts=1737298800 2025-W03  count=76.0 size=94.0
  g=110 ts=1737309600 2025-W03  count=76.0 size=92.0
  g=112 ts=1737331200 2025-W04  count=75.0 size=92.0
  g=117 ts=1737385200 2025-W04  count=75.0 size=92.0
  g=118 ts=1737396000 2025-W04  count=75.0 size=89.0
  g=119 ts=1737406800 2025-W04  count=75.0 size=90.0
  g=120 ts=1737417600 2025-W04  count=72.0 size=89.0
  g=122 ts=1737439200 2025-W04  count=72.0 size=86.0
  g=123 ts=1737450000 2025-W04  count=76.0 size=86.0
  g=125 ts=1737471600 2025-W04  count=81.0 size=85.0
  g=128 ts=1737504000 2025-W04  count=81.0 size=85.0
  g=129 ts=1737514800 2025-W04  count=83.0 size=88.0
  g=131 ts=1737536400 2025-W04  count=83.0 size=88.0
  g=132 ts=1737547200 2025-W04  count=83.0 size=92.0
  g=134 ts=1737568800 2025-W04  count=83.0 size=92.0
  g=137 ts=1737601200 2025-W04  count=83.0 size=96.0
  g=138 ts=1737612000 2025-W04  count=83.0 size=94.0
  g=142 ts=1737655200 2025-W04  count=83.0 size=94.0
  g=145 ts=1737687600 2025-W04  count=83.0 size=94.0
  g=146 ts=1737698400 2025-W04  count=80.0 size=94.0
  g=150 ts=1737741600 2025-W04  count=79.0 size=94.0
  g=154 ts=1737784800 2025-W04  count=78.0 size=93.0
  g=156 ts=1737806400 2025-W04  count=78.0 size=93.0
  g=159 ts=1737838800 2025-W04  count=78.0 size=90.0
  g=162 ts=1737871200 2025-W04  count=78.0 size=88.0
  g=164 ts=1737892800 2025-W04  count=78.0 size=85.0
  g=166 ts=1737914400 2025-W04  count=78.0 size=85.0
  g=170 ts=1737957600 2025-W05  count=78.0 size=84.0
  g=171 ts=1737968400 2025-W05  count=78.0 size=80.0
  g=173 ts=1737990000 2025-W05  count=75.0 size=80.0
  g=175 ts=1738011600 2025-W05  count=78.0 size=80.0
  g=178 ts=1738044000 2025-W05  count=78.0 size=80.0
  g=179 ts=1738054800 2025-W05  count=78.0 size=77.0
  g=180 ts=1738065600 2025-W05  count=74.0 size=77.0
  g=182 ts=1738087200 2025-W05  count=74.0 size=77.0
  g=185 ts=1738119600 2025-W05  count=74.0 size=77.0
  g=191 ts=1738184400 2025-W05  count=74.0 size=77.0
  g=192 ts=1738195200 2025-W05  count=76.0 size=77.0
  g=194 ts=1738216800 2025-W05  count=76.0 size=77.0
  g=197 ts=1738249200 2025-W05  count=77.0 size=77.0
  g=199 ts=1738270800 2025-W05  count=77.0 size=77.0
"
    );

    Ok(())
}
