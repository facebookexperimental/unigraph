// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use clap::Parser;
use unigraph_cli::UnigraphCLIContext;
use unigraph_cli::UnigraphCLISubcommand;
use unigraph_storage_core::TimelineID;

use crate::expand_tilde;

#[derive(Parser)]
pub struct Ingest {
    /// Path to the ingestion config file
    #[arg(long)]
    config: PathBuf,

    /// Maximum number of new commits to ingest (for testing)
    #[arg(long)]
    limit: Option<usize>,
}

impl UnigraphCLISubcommand for Ingest {
    async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> anyhow::Result<()> {
        let config_file = &self.config;
        let configs = unigraph_ingestion::load_ingestion_configs(config_file)?;

        ctx.eprintln_after_done(&format!(
            "Loaded {} ingestion config(s) from {}",
            configs.len(),
            config_file.display()
        ))?;

        let options = unigraph_ingestion::IngestionOptions { limit: self.limit };

        for (i, ingestion_config) in configs.iter().enumerate() {
            ctx.eprintln_after_done(&format!(
                "\n=== Ingestion config {}/{} ===",
                i + 1,
                configs.len()
            ))?;
            run_one_config(ingestion_config, &ctx.db, &options, task).await?;
        }

        Ok(())
    }
}

async fn run_one_config(
    config: &unigraph_ingestion::IngestionConfig,
    db: &unigraph_db::UnigraphDb,
    options: &unigraph_ingestion::IngestionOptions,
    task: &ll::Task,
) -> anyhow::Result<()> {
    let source = resolve_source(&config.source, db, task).await?;

    let mut cargo_builders = Vec::new();

    enum BuilderRef {
        Cargo(usize),
    }
    let mut builder_refs = Vec::new();
    let mut timeline_ids = Vec::new();

    for entry in &config.timelines {
        timeline_ids.push(TimelineID(entry.timeline_id.clone()));
        match &entry.builder {
            unigraph_ingestion::GraphBuilderConfig::Cargo {
                manifest_path,
                collect_timings,
                collect_sizes,
            } => {
                let idx = cargo_builders.len();
                cargo_builders.push(unigraph_ingestion::CargoGraphBuilder::new(
                    PathBuf::from(manifest_path),
                    *collect_timings,
                    *collect_sizes,
                ));
                builder_refs.push(BuilderRef::Cargo(idx));
            }
        }
    }

    let builders: Vec<unigraph_ingestion::TimelineBuilderConfig<'_>> = builder_refs
        .iter()
        .enumerate()
        .map(|(i, bref)| {
            let builder = match bref {
                BuilderRef::Cargo(idx) => {
                    unigraph_ingestion::Builder::FromRepo(&cargo_builders[*idx])
                }
            };
            unigraph_ingestion::TimelineBuilderConfig {
                timeline_id: timeline_ids[i].clone(),
                builder,
            }
        })
        .collect();

    let pipeline_config = unigraph_ingestion::IngestionPipelineConfig { source, builders };

    unigraph_ingestion::run_ingestion(&pipeline_config, db, options).await
}

async fn resolve_source(
    source_config: &unigraph_ingestion::IngestionSourceConfig,
    _db: &unigraph_db::UnigraphDb,
    _task: &ll::Task,
) -> anyhow::Result<unigraph_ingestion::IngestionSource> {
    match source_config {
        unigraph_ingestion::IngestionSourceConfig::Git {
            repo_path,
            main_branch,
        } => {
            let repo_path = expand_tilde(repo_path);
            let ns =
                unigraph_storage_core::ExternalIDNamespace(format!("{}/git", repo_path.display()));
            Ok(unigraph_ingestion::IngestionSource::Git {
                repo_path,
                main_branch: main_branch.clone(),
                external_id_namespace: ns,
            })
        }
    }
}
