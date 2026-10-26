//! Generates the C++ side of the bridge (headers and glue) with cxx-qt-build.
//! When built through CMake, cxx-qt-cmake tells it where to export headers.

fn main() {
    cxx_qt_build::CxxQtBuilder::new()
        .file("src/core_object.rs")
        .build();
}
