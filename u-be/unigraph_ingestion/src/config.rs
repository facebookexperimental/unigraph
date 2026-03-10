// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use unigraph_storage_core::ExternalIDNamespace;
use unigraph_storage_core::TimelineID;

use crate::graph_builder::Builder;

/// Source system configuration for ingestion.
///
/// Each variant carries its own `external_id_namespace` — the source
/// defines how external IDs are derived.
pub enum IngestionSource {
    /// Git repository source.
    Git {
        /// Path to the git repository on disk.
        repo_path: PathBuf,
        /// Name of the main branch (e.g. "main", "master").
        main_branch: String,
        /// Namespace for external ID mappings (typically "{repo_path}/git").
        external_id_namespace: ExternalIDNamespace,
    },
    /// Derives graphs from an existing timeline.
    ///
    /// Shares the same external ID namespace and GraphIDs as the source
    /// timeline — no new external IDs are allocated.
    AnotherTimeline {
        /// Timeline to read source graphs from.
        source_timeline_id: TimelineID,
        /// Namespace shared with the source timeline.
        external_id_namespace: ExternalIDNamespace,
    },
}

impl IngestionSource {
    pub fn external_id_namespace(&self) -> &ExternalIDNamespace {
        match self {
            Self::Git {
                external_id_namespace,
                ..
            } => external_id_namespace,
            Self::AnotherTimeline {
                external_id_namespace,
                ..
            } => external_id_namespace,
        }
    }
}

/// Configuration for a single timeline's graph builder.
pub struct TimelineBuilderConfig<'a> {
    /// Timeline ID to create/append to.
    pub timeline_id: TimelineID,
    /// Graph builder for this timeline.
    pub builder: Builder<'a>,
}

/// Top-level ingestion pipeline configuration.
pub struct IngestionPipelineConfig<'a> {
    /// Source system to ingest from.
    pub source: IngestionSource,
    /// One or more timeline+builder pairs. Each revision is processed by every builder,
    /// producing one frame per timeline per revision.
    pub builders: Vec<TimelineBuilderConfig<'a>>,
}
