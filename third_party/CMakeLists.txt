# Static libraries built from the vendored, unmodified Scintilla and Lexilla
# sources. Their warnings are silenced: we do not patch vendored code
# (third_party/README.md), so FERNY_WARNINGS_AS_ERRORS applies to ui/ only.

function(ferny_vendored_settings target)
    set_target_properties(${target} PROPERTIES POSITION_INDEPENDENT_CODE ON)
    if(MSVC)
        target_compile_options(${target} PRIVATE /W0)
        target_compile_definitions(${target} PRIVATE _CRT_SECURE_NO_DEPRECATE=1)
    else()
        target_compile_options(${target} PRIVATE -w)
    endif()
endfunction()

# --- Scintilla core + Qt platform layer (ScintillaEditBase) --------------------
set(SCI_DIR "${CMAKE_CURRENT_SOURCE_DIR}/scintilla")
file(GLOB SCI_CORE_SOURCES CONFIGURE_DEPENDS "${SCI_DIR}/src/*.cxx")

add_library(scintilla_qt STATIC
    ${SCI_CORE_SOURCES}
    "${SCI_DIR}/qt/ScintillaEditBase/PlatQt.cpp"
    "${SCI_DIR}/qt/ScintillaEditBase/ScintillaQt.cpp"
    "${SCI_DIR}/qt/ScintillaEditBase/ScintillaEditBase.cpp"
    "${SCI_DIR}/qt/ScintillaEditBase/PlatQt.h"
    "${SCI_DIR}/qt/ScintillaEditBase/ScintillaQt.h"
    "${SCI_DIR}/qt/ScintillaEditBase/ScintillaEditBase.h"
)
set_target_properties(scintilla_qt PROPERTIES AUTOMOC ON)
# SYSTEM: Ferny's code includes these headers and is built with warnings as
# errors; warnings inside vendored headers must not fail it. src/ is public
# because ScintillaEditBase.h includes core headers.
target_include_directories(scintilla_qt SYSTEM
    PUBLIC "${SCI_DIR}/include" "${SCI_DIR}/src" "${SCI_DIR}/qt/ScintillaEditBase"
)
# Static build: EXPORT_IMPORT_API empty on every platform.
target_compile_definitions(scintilla_qt
    PUBLIC SCINTILLA_QT=1 EXPORT_IMPORT_API=
    PRIVATE $<$<NOT:$<CONFIG:Debug>>:NDEBUG=1>
)
target_link_libraries(scintilla_qt PUBLIC Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Core5Compat)
ferny_vendored_settings(scintilla_qt)

# --- Lexilla (all lexers, linked statically) -----------------------------------
set(LEX_DIR "${CMAKE_CURRENT_SOURCE_DIR}/lexilla")
file(GLOB LEX_LIB_SOURCES CONFIGURE_DEPENDS "${LEX_DIR}/lexlib/*.cxx")
file(GLOB LEX_LEXER_SOURCES CONFIGURE_DEPENDS "${LEX_DIR}/lexers/*.cxx")

add_library(lexilla STATIC
    "${LEX_DIR}/src/Lexilla.cxx"
    ${LEX_LIB_SOURCES}
    ${LEX_LEXER_SOURCES}
)
target_include_directories(lexilla SYSTEM
    PUBLIC "${LEX_DIR}/include" "${SCI_DIR}/include"
    PRIVATE "${LEX_DIR}/lexlib"
)
target_compile_definitions(lexilla PRIVATE $<$<NOT:$<CONFIG:Debug>>:NDEBUG=1>)
ferny_vendored_settings(lexilla)
