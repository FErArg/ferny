# Ferny — top-level build. Builds the Qt application in ui/ and drives Cargo
# through Corrosion (via cxx-qt-cmake) for the Rust bridge.
#
#   cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug
#   cmake --build build
#   ctest --test-dir build --output-on-failure
#
# Configuring never downloads anything: Corrosion and cxx-qt-cmake are
# vendored in third_party/ and FetchContent is fully disconnected. Cargo
# fetches the crates pinned in Cargo.lock from crates.io.

cmake_minimum_required(VERSION 3.28)

project(Ferny
    VERSION 0.6.17
    DESCRIPTION "Geany-inspired programmer's editor"
    LANGUAGES CXX
)

# Embed the source revision in the About dialog. A detached or exported source
# tree without Git metadata reports "unknown" rather than failing to configure.
find_package(Git QUIET)
if(GIT_FOUND)
    execute_process(
        COMMAND "${GIT_EXECUTABLE}" rev-parse --short=12 HEAD
        WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}"
        OUTPUT_VARIABLE FERNY_GIT_COMMIT
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET
    )
endif()
if(NOT FERNY_GIT_COMMIT)
    set(FERNY_GIT_COMMIT "unknown")
endif()

option(FERNY_WARNINGS_AS_ERRORS "Treat compiler warnings in Ferny's own C++ code as errors" OFF)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Debug CACHE STRING "Build type" FORCE)
endif()

# Rust static libraries use the release MSVC runtime; everything linked with
# them must too (see third_party/cxx-qt-cmake/cmake/CxxQt.cmake).
if(MSVC)
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")
endif()

include(CTest)

# Qt: API floor 6.8 (docs/DEPENDENCIES.md). Core5Compat is required by
# Scintilla's Qt layer (QTextCodec); Network provides QLocalServer for the
# single instance.
# LinguistTools: lupdate/lrelease for the embedded translations (H1.6b).
find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Widgets Network Core5Compat PrintSupport LinguistTools)

# --- Rust bridge ---------------------------------------------------------------
# Use the vendored Corrosion instead of the network fetch in CxxQt.cmake, and
# fail instead of downloading if that ever changes.
set(FETCHCONTENT_FULLY_DISCONNECTED ON)
set(FETCHCONTENT_SOURCE_DIR_CORROSION "${CMAKE_CURRENT_SOURCE_DIR}/third_party/corrosion")
set(CORROSION_BUILD_TESTS OFF CACHE BOOL "Build Corrosion's own tests" FORCE)
add_subdirectory(third_party/cxx-qt-cmake EXCLUDE_FROM_ALL)

cxx_qt_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    CRATES ferny_bridge
    LOCKED
    QT_MODULES Qt::Core
)

# --- Vendored C++ (third_party/) -----------------------------------------------
add_subdirectory(third_party)

# --- Application ---------------------------------------------------------------
add_subdirectory(ui)
