//! Core types, paths, filesystem helpers, schema, and manifests shared by every other crate.
#![forbid(unsafe_code)]

pub mod checkout;
pub mod error;
pub mod fs;
pub mod manifest;
pub mod paths;
pub mod pyrepr;
pub mod schema;
pub mod shlex;
pub mod stow;
pub mod time;
pub mod toml_emit;
#[cfg(test)]
mod toml_emit_tests;
pub mod traits;
