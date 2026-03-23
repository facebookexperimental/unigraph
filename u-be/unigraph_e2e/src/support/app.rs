// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Test application setup — in-memory Unigraph instance for e2e tests.

use std::sync::Arc;

use anyhow::Result;
use unigraph_app::Unigraph;
use unigraph_app::UnigraphRequest;
use unigraph_app::UnigraphResponse;
use unigraph_db::UnigraphDb;
use unigraph_storage_sqlite::SqliteStorage;

/// A fully initialized Unigraph app for testing.
pub struct TestApp {
    pub app: Unigraph,
    pub task: ll::Task,
}

impl TestApp {
    /// Dispatch an RPC request through the app layer.
    pub async fn rpc(&self, req: UnigraphRequest) -> Result<UnigraphResponse> {
        self.app.exec_rpc(req, &self.task).await
    }
}

/// Create an in-memory Unigraph app for testing.
pub fn init_app() -> TestApp {
    let sqlite = Arc::new(SqliteStorage::new_in_memory().unwrap());
    let db = UnigraphDb::new(sqlite.clone(), sqlite.clone());
    let task = ll::Task::create_new("e2e");
    TestApp {
        app: Unigraph::new(db),
        task,
    }
}
