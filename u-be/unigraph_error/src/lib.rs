// Copyright (c) Meta Platforms, Inc. and affiliates.
pub mod error;
pub mod format;
pub mod macros;

pub use error::UnigraphError;
pub use format::format_for_user;
pub use format::format_strip_ansi;
pub use format::format_with_colors;
pub use format::into_unigraph_error;
pub use format::to_json;

pub const SEPARATOR: &str =
    "~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~";

pub type UnigraphResult<T> = std::result::Result<T, UnigraphError>;

#[cfg(test)]
mod tests;
