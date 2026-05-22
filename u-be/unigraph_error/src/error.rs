// Copyright (c) Meta Platforms, Inc. and affiliates.
use std::error::Error;
use std::fmt::Write;

/// This is a centralized Error struct for unigraph code that can be used
/// cross-platform. It can be serialized to and from JSON and passed cross php
/// or javascript boundary.
///
/// Error (or this crate) also provides a set of formatters that can format it
/// with CLI colors or without, or serialize it to JSON for passing up the stack
/// (e.g. PHP caller)
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct UnigraphError {
    pub display: String,
    pub debug: Option<String>,
}

impl Error for UnigraphError {}

impl UnigraphError {
    pub fn new<DS: Into<String>, DB: Into<String>>(display: DS, debug: Option<DB>) -> Self {
        UnigraphError {
            display: display.into(),
            debug: debug.map(|d| d.into()),
        }
    }

    /// Append more debugging info to an already existing error.
    /// This info will go to the bottom of the error text
    /// separated with "~~~~~" line
    pub fn append_debug_info(&mut self, s: &str) {
        let mut result = String::new();
        if let Some(existing) = self.debug.take() {
            writeln!(result, "{existing}").unwrap();
            writeln!(result, "{}", crate::SEPARATOR).unwrap();
        }
        writeln!(result, "{s}").unwrap();
        self.debug = Some(result);
    }
}

impl From<anyhow::Error> for UnigraphError {
    fn from(error: anyhow::Error) -> Self {
        super::into_unigraph_error(&error)
    }
}
