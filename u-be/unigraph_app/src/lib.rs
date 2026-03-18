// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Top-level application struct for Unigraph.
//!
//! [`Unigraph`] wraps [`UnigraphDb`] and will eventually hold in-memory
//! caches, app-level configuration, and other cross-cutting concerns.

use unigraph_db::UnigraphDb;

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
}
