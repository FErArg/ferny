//! cxx-qt bridge exposing the Rust core to the Qt UI. The only crate allowed to use unsafe code.
//!
//! Milestone H0.3: one `FernyCore` QObject that loads documents through
//! `ferny-core`. The UI receives UTF-8 bytes or a status code, never a path
//! or error text produced here (user-facing messages are translated in the
//! UI, `docs/DESIGN.md` §6).
#![deny(unsafe_code)]

// The cxx-qt bridge macro expands to FFI glue; see the `// SAFETY:` comment
// on the bridge in `core_object.rs`. No other module may allow unsafe code.
// Search functions mirror Qt-style invokables with output parameters
// (range, replacement), which exceeds clippy's argument count; the bridge
// macro does not accept attributes on individual items.
#[allow(unsafe_code, clippy::too_many_arguments)]
pub mod core_object;
