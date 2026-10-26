//! Settings schema, TOML layering and the Geany importer.
//!
//! Settings (`settings`) and keybindings (`keybindings`) since H1.5a; the
//! Geany importer (`geany`) since H1.5b; projects and the `.geany`
//! project importer (`project`) since H2.5a; templates and snippets
//! (`templates`) since H2.5b.
#![forbid(unsafe_code)]

pub mod geany;
pub mod keybindings;
pub mod project;
pub mod settings;
pub mod templates;
