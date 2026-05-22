// Copyright (c) Meta Platforms, Inc. and affiliates.

pub mod graph;
pub mod stats;
pub mod timelines;

use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Result;
use unigraph_db::UnigraphDb;

pub struct UnigraphCLIContext {
    pub db: UnigraphDb,
    pub sqlite_path: PathBuf,
    deferred_stdout: Arc<Mutex<String>>,
    deferred_stderr: Arc<Mutex<String>>,
}

impl UnigraphCLIContext {
    pub fn new(db: UnigraphDb, sqlite_path: PathBuf) -> Self {
        Self {
            db,
            sqlite_path,
            deferred_stdout: Arc::new(Mutex::new(String::new())),
            deferred_stderr: Arc::new(Mutex::new(String::new())),
        }
    }

    /// Write to stdout after the command is done and all tasks finished,
    /// avoiding interference with the interactive task tree.
    pub fn println_after_done(&self, s: &str) -> Result<()> {
        let mut buf = self.deferred_stdout.lock().unwrap();
        writeln!(buf, "{}", s)?;
        Ok(())
    }

    pub fn eprintln_after_done(&self, s: &str) -> Result<()> {
        let mut buf = self.deferred_stderr.lock().unwrap();
        writeln!(buf, "{}", s)?;
        Ok(())
    }

    pub fn take_deferred_stdout(&self) -> String {
        std::mem::take(&mut *self.deferred_stdout.lock().unwrap())
    }

    pub fn take_deferred_stderr(&self) -> String {
        std::mem::take(&mut *self.deferred_stderr.lock().unwrap())
    }

    pub fn flush_deferred(&self) {
        let stderr = self.deferred_stderr.lock().unwrap();
        if !stderr.is_empty() {
            eprint!("{}", &*stderr);
        }
        drop(stderr);

        let stdout = self.deferred_stdout.lock().unwrap();
        if !stdout.is_empty() {
            print!("{}", &*stdout);
        }
    }
}

#[expect(
    async_fn_in_trait,
    reason = "only used with concrete types, no dyn dispatch"
)]
pub trait UnigraphCLISubcommand {
    async fn run(&self, ctx: &UnigraphCLIContext, task: &ll::Task) -> Result<()>;
}
