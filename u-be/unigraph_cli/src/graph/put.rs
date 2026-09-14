// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::MapGraph;
use unigraph_storage_core::AdjacentDeltasConfig;
use unigraph_storage_core::GraphID;
use unigraph_storage_core::GraphKey;
use unigraph_storage_core::GraphTimeKey;
use unigraph_storage_core::TimelineConfig;
use unigraph_storage_core::TimelineID;
use unigraph_storage_core::TimelineSchema;
use unigraph_storage_core::Timestamp;

use crate::UnigraphCLIContext;

const SINKHOLE_TIMELINE: &str = "sinkhole";

/// Store a graph JSON file as a frame in a timeline.
///
/// Reads the graph from a JSON file (auto-detects MapGraph and
/// ArrayGraphSerializable formats) and stores it, printing the resulting
/// graph key and stats as JSON. Without `--timeline` the graph goes to the
/// `sinkhole` timeline, which is created on demand.
///
/// Examples:
///
/// ```sh
/// # Quick-store into the sinkhole timeline
/// unigraph graph put /tmp/graph.json
///
/// # Store into a specific timeline with an explicit ID and timestamp
/// unigraph graph put /tmp/graph.json --timeline my_timeline --graph-id 1 \
///     --timestamp '2025-01-01T00:00:00Z'
///
/// # Store as delta from another graph
/// unigraph graph put /tmp/graph.json --timeline my_timeline --graph-id 2 \
///     --delta-from my_timeline~1
/// ```
#[derive(Parser, Debug)]
pub struct GraphPut {
    /// Path to a JSON file containing the graph (MapGraph or ArrayGraphSerializable)
    file: PathBuf,

    /// Timeline to store into. Must already exist. Defaults to `sinkhole`,
    /// which is created on demand
    #[arg(long)]
    timeline: Option<TimelineID>,

    /// Graph ID. Defaults to a freshly generated unique ID
    #[arg(long)]
    graph_id: Option<i64>,

    /// Timestamp in RFC 3339 format. Defaults to now
    #[arg(long)]
    timestamp: Option<Timestamp>,

    /// Store as a delta from this base graph key (`timeline_id~graph_id`)
    #[arg(long)]
    delta_from: Option<GraphKey>,
}

impl GraphPut {
    pub async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let timeline_id = self.resolve_timeline(ctx, task).await?;
        task.data("timeline", timeline_id.0.as_str());

        let ags = parse_graph_file(&self.file, task)?;
        let key = self.graph_time_key(&timeline_id, ctx, task).await?;
        self.store(ctx, &key, &ags, task).await?;
        self.print_stored(&key, &ags)
    }

    async fn resolve_timeline(
        &self,
        ctx: &UnigraphCLIContext,
        task: &ll::Task,
    ) -> anyhow::Result<TimelineID> {
        let Some(timeline) = &self.timeline else {
            return ensure_sinkhole_timeline(ctx, task).await;
        };
        Ok(timeline.clone())
    }

    async fn graph_time_key(
        &self,
        timeline_id: &TimelineID,
        ctx: &UnigraphCLIContext,
        task: &ll::Task,
    ) -> anyhow::Result<GraphTimeKey> {
        let graph_id = match self.graph_id {
            Some(id) => GraphID(id),
            None => GraphID(ctx.db.utility.gen_uniq_id(task).await?),
        };
        Ok(GraphTimeKey {
            timeline_id: timeline_id.clone(),
            timestamp: self.timestamp.unwrap_or_else(Timestamp::now),
            graph_id,
        })
    }

    async fn store(
        &self,
        ctx: &UnigraphCLIContext,
        key: &GraphTimeKey,
        ags: &ArrayGraphSerializable,
        task: &ll::Task,
    ) -> anyhow::Result<()> {
        let Some(from_key) = &self.delta_from else {
            return ctx
                .db
                .graph
                .store(key, ags, None, task)
                .await
                .context("Failed to store graph");
        };
        ctx.db
            .graph
            .store_as_delta_from(key, ags, from_key, task)
            .await
            .context("Failed to store graph as delta")
    }

    fn print_stored(&self, key: &GraphTimeKey, ags: &ArrayGraphSerializable) -> anyhow::Result<()> {
        let result = serde_json::json!({
            "graph_key": key.graph_key().to_string(),
            "timeline": key.timeline_id.0,
            "graph_id": key.graph_id.0,
            "timestamp": key.timestamp,
            "file": self.file.display().to_string(),
            "delta_from": self.delta_from.as_ref().map(GraphKey::to_string),
            "stats": {
                "nodes": ags.node_names_ordered.len(),
                "edges": ags.edges.edges_len(),
            },
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
        Ok(())
    }
}

pub fn parse_graph_file(path: &Path, task: &ll::Task) -> anyhow::Result<ArrayGraphSerializable> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read graph file: {}", path.display()))?;
    task.data("file_size_bytes", bytes.len());

    if let Ok(ags) = ArrayGraphSerializable::from_json_bytes(&bytes) {
        return Ok(ags);
    }

    let map_graph = MapGraph::from_json_bytes(&bytes)
        .context("Failed to parse graph file as MapGraph or ArrayGraphSerializable")?;
    map_graph
        .to_array_graph_serializable()
        .context("Failed to convert MapGraph to ArrayGraphSerializable")
}

async fn ensure_sinkhole_timeline(
    ctx: &UnigraphCLIContext,
    task: &ll::Task,
) -> anyhow::Result<TimelineID> {
    let tid = TimelineID(SINKHOLE_TIMELINE.to_string());
    if ctx.db.timelines.list(task).await?.contains(&tid) {
        return Ok(tid);
    }
    let config = TimelineConfig {
        schema: TimelineSchema::AdjacentDeltas(AdjacentDeltasConfig {}),
        external_id_namespace: None,
        blob_storage: Default::default(),
        store_metric_history: None,
    };
    ctx.db
        .timelines
        .create(&tid, &config, task)
        .await
        .context("Failed to create sinkhole timeline")?;
    Ok(tid)
}
