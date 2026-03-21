// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Top-level application struct for Unigraph.
//!
//! [`Unigraph`] wraps [`UnigraphDb`] and will eventually hold in-memory
//! caches, app-level configuration, and other cross-cutting concerns.

use anyhow::Result;
use unigraph_db::UnigraphDb;
use unigraph_rpc::RpcExec;

mod rpc_req;
mod rpc_types;

pub use rpc_types::*;

/// The Unigraph application — wraps the database and (eventually) caches.
///
/// Constructed by the CLI or web service after setting up storage backends.
#[derive(Clone)]
pub struct Unigraph {
    pub db: UnigraphDb,
}

impl Unigraph {
    pub fn new(db: UnigraphDb) -> Self {
        Self { db }
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
        }
    }
}

unigraph_rpc::define_rpc_for_exec! {
    pub Unigraph {
        PutConfigs(PutConfigsInput) -> PutConfigsOutput,
        GetConfigs(GetConfigsInput) -> GetConfigsOutput,
        GraphQuery(GraphQueryInput) -> GraphQueryOutput,
        ListTimelines(ListTimelinesInput) -> ListTimelinesOutput,
        SelectFrames(SelectFramesInput) -> SelectFramesOutput,
    }
}
