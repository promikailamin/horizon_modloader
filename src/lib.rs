//! `.hmmod` — a precompiled Horizon mod package.
//!
//! Horizon's `.hnmod` files contain Rust source that the launcher compiles into
//! a custom `horizon-client` for every player. An `.hmmod` moves that compile
//! to the mod author: it carries the already-built client, so installing a mod
//! is only unpacking and launching.
//!
//! This crate implements the archive format, its validation, a local mod store
//! and the `.hnmod` -> `.hmmod` conversion pipeline.

pub mod convert;
pub mod format;
pub mod hnmod;
pub mod runtime;
pub mod store;

pub use format::{HmmodManifest, ManifestError};
pub use store::{InstalledMod, LoaderHome};
