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
//!
//! // Generate TypeScript declaration
//! let type_decl = User::to_type_decl();
//! println!("{}", type_decl.export_typescript());
//!
//! // Generate Flow declaration
//! println!("{}", type_decl.export_flow());
//! ```

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
