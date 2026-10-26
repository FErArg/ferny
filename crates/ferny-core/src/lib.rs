//! Documents, rope mirror, sessions, encodings, safe-save, IPC, workspace trust and the command registry.
//!
//! Implemented: the command registry (H0.5) and documents — encodings,
//! line endings, bounded reads, atomic saves, external changes (H1.1); command-line
//! locations, single-instance requests and the session (H1.2); directory
//! listings for the Files sidebar (H1.6a); workspace trust (H2.1); the rest
//! arrives in later milestones.
#![forbid(unsafe_code)]

pub mod backup;
pub mod browse;
pub mod command;
pub mod document;
pub mod editing;
pub mod editorconfig;
pub mod export;
pub mod fsio;
pub mod git;
pub mod glob;
pub mod ipc;
pub mod location;
pub mod log;
pub mod markdown;
pub mod search;
pub mod session;
pub mod spell;
pub mod text;
pub mod trust;

/// Ferny's version, as recorded in the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
