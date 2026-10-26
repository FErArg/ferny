# crates/ferny-bridge

cxx-qt bridge exposing the Rust core to the Qt UI as QObjects. The only crate that knows about Qt.

Package name `ferny_bridge` (underscore): cxx-qt-cmake 0.10 exports the
generated headers under Corrosion's target name, which never has hyphens,
while cxx-qt-build uses the package name. Built as a static library by
CMake (`cxx_qt_import_crate`); `build.rs` needs `qmake` (Qt installed).

H0.3: `ferny::FernyCore` with `loadDocument(path, contents) -> LoadStatus`
and `version()`; command registry access (`menuIds`, `menuTitle`,
`menuItems`, `commandTitle`, `commandShortcuts`, `invokeCommand`) and the
`commandRequested(id)` signal (H0.5). Unsafe code is allowed only on the `core_object` module
(the cxx-qt macro expands to FFI glue); see its `// SAFETY:` comment.
