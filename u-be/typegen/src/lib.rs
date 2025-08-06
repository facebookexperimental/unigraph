#![allow(clippy::uninlined_format_args)]
#![allow(clippy::collapsible_if)]

//! # TypeGen
//!
//! A library for generating TypeScript and Flow.js type definitions from Rust structs.
//!
//! ## Usage
//!
//! ```rust
//! use typegen::TypeGen;
//! use typegen::TypeGenDeclTrait;
//!
//! #[derive(TypeGen)]
//! struct User {
//!     id: u64,
//!     name: String,
//! }
//! ```
//! Add a typegen.toml file somewhere in any of the parent directories
//! of your source file.
//! Now running tests with TYPEGEN=1 will generate definitions for languages
//! specified in the typegen.toml file.

mod config;
mod flow;
mod types;
mod typescript;

pub use config::*;
pub use flow::*;
// Always export the derive macro
pub use typegen_derive::TypeGen;
pub use types::*;
pub use typescript::*;
