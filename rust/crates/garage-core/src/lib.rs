//! Core types, paths, filesystem helpers, schema, and manifests shared by every other crate.
#![forbid(unsafe_code)]

pub mod error;
pub mod fs;
pub mod manifest;
pub mod paths;
pub mod pyrepr;
pub mod schema;
pub mod toml_emit;
pub mod traits;
