// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Top-level application struct for Unigraph.
//!
//! [`Unigraph`] wraps [`UnigraphDb`] and will eventually hold in-memory
//! caches, app-level configuration, and other cross-cutting concerns.

use anyhow::Result;
use unigraph_db::UnigraphDb;
use unigraph_rpc::RpcExec;

mod graph_cache;
mod rpc_req;
mod rpc_types;

pub use graph_cache::GraphCache;
pub use rpc_types::*;

/// The Unigraph application — wraps the database, caches, and cross-cutting concerns.
///
/// Constructed by the CLI or web service after setting up storage backends.
#[derive(Clone)]
pub struct Unigraph {
    pub db: UnigraphDb,
    pub graph_cache: GraphCache,
}

impl Unigraph {
    pub fn new(db: UnigraphDb) -> Self {
        let graph_cache = GraphCache::new(db.clone(), 64);
        Self { db, graph_cache }
    }

    pub async fn exec_rpc(
        &self,
        req: UnigraphRequest,
        task: &ll::Task,
    ) -> Result<UnigraphResponse> {
        match req {
            UnigraphRequest::PutConfigs(input) => {
                Ok(UnigraphResponse::PutConfigs(input.exec(self, task).await?))
            }
            UnigraphRequest::GetConfigs(input) => {
                Ok(UnigraphResponse::GetConfigs(input.exec(self, task).await?))
            }
            UnigraphRequest::GraphQuery(input) => {
                Ok(UnigraphResponse::GraphQuery(input.exec(self, task).await?))
            }
            UnigraphRequest::ListTimelines(input) => Ok(UnigraphResponse::ListTimelines(
                input.exec(self, task).await?,
            )),
            UnigraphRequest::SelectFrames(input) => Ok(UnigraphResponse::SelectFrames(
                input.exec(self, task).await?,
            )),
            UnigraphRequest::ExploreGraph(input) => Ok(UnigraphResponse::ExploreGraph(
                input.exec(self, task).await?,
            )),
            UnigraphRequest::SearchNodes(input) => {
                Ok(UnigraphResponse::SearchNodes(input.exec(self, task).await?))
            }
        }
    }
}

/// Call an RPC method and unwrap the response in one step.
///
/// Wraps the input in the correct `UnigraphRequest` variant, calls `.rpc()`,
/// checks for `Error`, and extracts the expected output variant.
///
/// The first argument must have an `async fn rpc(UnigraphRequest) -> Result<UnigraphResponse>`.
///
/// ```ignore
/// let put = call_rpc!(t, PutConfigs(PutConfigsInput {
///     traversal_configs: vec![sample_tvc()],
///     graph_query_configs: vec![sample_gqc()],
/// }));
/// // put: PutConfigsOutput
/// ```
#[macro_export]
macro_rules! call_rpc {
    ($ctx:expr, $variant:ident($input:expr)) => {{
        let resp = $ctx.rpc($crate::UnigraphRequest::$variant($input)).await?;
        match resp {
            $crate::UnigraphResponse::Error(err) => ::anyhow::Result::Err(err.into_anyhow()),
            $crate::UnigraphResponse::$variant(output) => ::anyhow::Result::Ok(output),
            other => ::anyhow::bail!(
                "unexpected response: expected {}, got {}",
                stringify!($variant),
                other.variant_name()
            ),
        }?
    }};
}

unigraph_rpc::define_rpc_for_exec! {
    pub Unigraph {
        PutConfigs(PutConfigsInput) -> PutConfigsOutput,
        GetConfigs(GetConfigsInput) -> GetConfigsOutput,
        GraphQuery(GraphQueryInput) -> GraphQueryOutput,
        ListTimelines(ListTimelinesInput) -> ListTimelinesOutput,
        SelectFrames(SelectFramesInput) -> SelectFramesOutput,
        ExploreGraph(ExploreGraphInput) -> ExploreGraphOutput,
        SearchNodes(SearchNodesInput) -> SearchNodesOutput,
    }
}
