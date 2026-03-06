// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use unigraph_storage_core::ExternalIDNamespace;
use unigraph_storage_core::TimelineID;

use crate::graph_builder::GraphBuilder;

/// Source system configuration for ingestion.
pub enum IngestionSource {
    /// Git repository source.
    Git {
        /// Path to the git repository on disk.
        repo_path: PathBuf,
        /// Name of the main branch (e.g. "main", "master").
        main_branch: String,
    },
    // Future: Hg { ... }
}

/// Configuration for a single timeline's graph builder.
pub struct TimelineBuilderConfig<'a> {
    /// Timeline ID to create/append to.
    pub timeline_id: TimelineID,
    /// Graph builder for this timeline.
    pub builder: &'a dyn GraphBuilder,
}

/// Top-level ingestion pipeline configuration.
pub struct IngestionPipelineConfig<'a> {
    /// Source system to ingest from.
    pub source: IngestionSource,
    /// Namespace for external ID mappings (shared across timelines from the same source).
    pub external_id_namespace: ExternalIDNamespace,
    /// One or more timeline+builder pairs. Each revision is processed by every builder,
    /// producing one frame per timeline per revision.
    pub builders: Vec<TimelineBuilderConfig<'a>>,
}
