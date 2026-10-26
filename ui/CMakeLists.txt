# The Ferny Qt application.

# WIN32: GUI subsystem, so no console window opens when Ferny starts from
# Explorer (main.cpp attaches to the parent console for command-line output).
# Qt links its WinMain entry point here; the keyword is ignored elsewhere.
qt_add_executable(ferny WIN32
    src/main.cpp
    src/mainwindow.cpp
    src/mainwindow.h
    src/messagewindow.cpp
    src/messagewindow.h
    src/preferences.cpp
    src/preferences.h
    src/core_strings.cpp
    src/fernyeditor.cpp
    src/fernyeditor.h
    src/filebrowser.cpp
    src/filebrowser.h
    src/findbar.cpp
    src/findbar.h
    src/singleinstance.cpp
    src/singleinstance.h
)
set_target_properties(ferny PROPERTIES AUTOMOC ON)

# macOS application bundle for packaging (H1.6c, scripts/package-macos.sh).
# Off by default so development builds keep build/ui/ferny.
option(FERNY_MACOS_BUNDLE "Build Ferny.app on macOS" OFF)
if(APPLE AND FERNY_MACOS_BUNDLE)
    set_target_properties(ferny PROPERTIES
        MACOSX_BUNDLE TRUE
        OUTPUT_NAME ferny
        MACOSX_BUNDLE_INFO_PLIST "${PROJECT_SOURCE_DIR}/packaging/macos/Info.plist.in")
endif()

# Application branding and Lucide icons, embedded as Qt resources.
qt_add_resources(ferny "ferny_images"
    PREFIX "/"
    BASE "${PROJECT_SOURCE_DIR}/imagenes"
    FILES
        "${PROJECT_SOURCE_DIR}/imagenes/ferny-icon-app.png"
        "${PROJECT_SOURCE_DIR}/imagenes/ferny-logo-removebg-preview.png"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/file-plus.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/folder-open.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/save.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/printer.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/undo-2.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/redo-2.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/search.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/hammer.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/play.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/info.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/chevron-down.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/chevron-up.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/code.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/square.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/scissors.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/clipboard.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/arrow-left.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/arrow-right.svg"
        "${PROJECT_SOURCE_DIR}/imagenes/lucide/copy.svg"
)

# Translations (H1.6b): ferny_es.ts compiled by lrelease and embedded under
# ":/i18n"; never loaded from disk. `cmake --build build --target
# update_translations` refreshes the .ts files with lupdate.
qt_add_translations(ferny
    TS_FILES "${CMAKE_CURRENT_SOURCE_DIR}/translations/ferny_es.ts"
    RESOURCE_PREFIX "/i18n"
    LUPDATE_OPTIONS -no-obsolete -locations none
)

# Windows: icon of the executable (Explorer, taskbar before Qt starts).
if(WIN32)
    target_sources(ferny PRIVATE "${PROJECT_SOURCE_DIR}/packaging/windows/ferny.rc")
endif()
# QT_NO_KEYWORDS is not set: ScintillaEditBase.h uses `signals`/`slots`.
target_compile_definitions(ferny PRIVATE
    QT_NO_CAST_FROM_ASCII
    QT_NO_CAST_TO_ASCII
    FERNY_GIT_COMMIT="${FERNY_GIT_COMMIT}"
)
# ferny_bridge: the Rust crate in crates/ferny-bridge, imported by
# cxx_qt_import_crate.
target_link_libraries(ferny PRIVATE
    ferny_bridge
    scintilla_qt
    lexilla
    Qt6::Core
    Qt6::Gui
    Qt6::Widgets
    Qt6::PrintSupport
    Qt6::Network
)

# The bridge target exports generated CXX-Qt headers. Compile them as system
# headers so third-party/generated warnings do not become Ferny-source errors;
# keep all warning flags and -Werror enabled for this target's own sources.
if(TARGET ferny_bridge)
    get_target_property(FERNY_BRIDGE_INCLUDE_DIRS ferny_bridge INTERFACE_INCLUDE_DIRECTORIES)
    if(FERNY_BRIDGE_INCLUDE_DIRS)
        target_include_directories(ferny SYSTEM PRIVATE ${FERNY_BRIDGE_INCLUDE_DIRS})
    endif()
endif()

if(MSVC)
    target_compile_options(ferny PRIVATE /W4 /permissive- /utf-8)
    if(FERNY_WARNINGS_AS_ERRORS)
        target_compile_options(ferny PRIVATE /WX)
    endif()
else()
    target_compile_options(ferny PRIVATE -Wall -Wextra -Wpedantic -Wshadow -Wnon-virtual-dtor)
    if(FERNY_WARNINGS_AS_ERRORS)
        target_compile_options(ferny PRIVATE -Werror)
    endif()
endif()

# --- Smoke tests (headless, offscreen platform) -------------------------------
if(BUILD_TESTING)
    set(FIXTURES "${PROJECT_SOURCE_DIR}/test/fixtures/ui")
    set(ROUNDTRIP "${CMAKE_CURRENT_BINARY_DIR}/roundtrip")
    set(SMOKE_ENV "QT_QPA_PLATFORM=offscreen")
    file(MAKE_DIRECTORY "${ROUNDTRIP}")

    # ferny_smoke(<name> <regex> <args...>): runs `ferny --smoke-test <args>`
    # and passes if the output matches <regex>.
    function(ferny_smoke name regex)
        add_test(NAME ${name} COMMAND ferny --smoke-test ${ARGN})
        set_tests_properties(${name} PROPERTIES
            ENVIRONMENT "${SMOKE_ENV}" PASS_REGULAR_EXPRESSION "${regex}")
    endfunction()

    ferny_smoke(ui_smoke_start
        "smoke: toolbar=.* icons=yes")
    ferny_smoke(ui_smoke_toolbar_commands
        "smoke: toolbar=.* icons=yes")
    ferny_smoke(ui_smoke_toolbar_dispatch
        "smoke: toolbar=.* icons=yes"
        "${FIXTURES}/hello.cpp" --smoke-command search.find)
    ferny_smoke(ui_smoke_open_cpp
        "smoke: ok lexer=cpp styled=yes length=[0-9]+ encoding=UTF-8 bom=no eol=LF readonly=no"
        "${FIXTURES}/hello.cpp")
    ferny_smoke(ui_smoke_open_rust "smoke: ok lexer=rust styled=yes .* filetype=rust" "${FIXTURES}/hello.rs")

    # Filetypes from Geany: by pattern, by shebang, custom ones; schemes.
    ferny_smoke(ui_smoke_filetype_shebang "lexer=python styled=yes .* filetype=python"
        "${FIXTURES}/python-script")
    ferny_smoke(ui_smoke_filetype_makefile "lexer=makefile .* filetype=makefile"
        "${FIXTURES}/Makefile")
    ferny_smoke(ui_smoke_filetype_custom "lexer=cpp .* filetype=arduino" "${FIXTURES}/sketch.ino")
    ferny_smoke(ui_smoke_scheme_default "scheme=default back=ffffff" "${FIXTURES}/hello.cpp")
    ferny_smoke(ui_smoke_scheme_darcula "scheme=darcula back=3b3b3b"
        --color-scheme darcula "${FIXTURES}/hello.cpp")
    ferny_smoke(ui_smoke_scheme_unknown "scheme=default back=ffffff"
        --color-scheme no-such-scheme "${FIXTURES}/hello.cpp")
    ferny_smoke(ui_smoke_open_latin1 "encoding=windows-1252 bom=no eol=LF readonly=no"
        "${FIXTURES}/latin1.txt")
    ferny_smoke(ui_smoke_open_utf16 "encoding=UTF-16LE bom=yes eol=CRLF readonly=no"
        "${FIXTURES}/utf16le-bom.txt")
    ferny_smoke(ui_smoke_open_bom_crlf "encoding=UTF-8 bom=yes eol=CRLF readonly=no"
        "${FIXTURES}/utf8-bom-crlf.txt")

    # Several files and positions (`file:line:col`, `+line file`).
    ferny_smoke(ui_smoke_tabs_and_position "tabs=2 cursor=4:5"
        "${FIXTURES}/hello.rs" "${FIXTURES}/hello.cpp:4:5")
    ferny_smoke(ui_smoke_plus_line "tabs=1 cursor=4:1" +4 "${FIXTURES}/hello.cpp")
    ferny_smoke(ui_smoke_same_file_once "tabs=1" "${FIXTURES}/hello.cpp" "${FIXTURES}/../ui/hello.cpp")

    # Session: save two files, then restore them in a second run.
    set(SESSION "${CMAKE_CURRENT_BINARY_DIR}/session-test/session.toml")
    # Removed before every run, not only at configure time, so ctest can
    # be run again.
    add_test(NAME ui_session_clean COMMAND ${CMAKE_COMMAND} -E rm -f "${SESSION}")
    set_tests_properties(ui_session_clean PROPERTIES FIXTURES_SETUP session_clean)
    ferny_smoke(ui_session_save "smoke: scribble loaded=0.*smoke: session saved"
        --session-file "${SESSION}" --smoke-scribble "notes ñ"
        "${FIXTURES}/hello.rs" "${FIXTURES}/hello.cpp:2:3")
    set_tests_properties(ui_session_save PROPERTIES FIXTURES_SETUP session
        FIXTURES_REQUIRED session_clean)
    # The Scribble (8 UTF-8 bytes) and the dock layout come back too (H1.6a).
    ferny_smoke(ui_session_restore "smoke: ok icon=yes logo=yes tabs=2 .*scribble loaded=8"
        --session-file "${SESSION}")
    set_tests_properties(ui_session_restore PROPERTIES FIXTURES_REQUIRED session)

    # UI shell (H1.6a): status bar fields, Documents and Files sidebars.
    ferny_smoke(ui_status_bar
        "smoke: status Line 1/9, Col 1[|]Sel 177[|]Spaces: 4[|]UTF-8[|]LF[|]C[+][+][|]Modified"
        --smoke-type "ñ" --smoke-command edit.select_all "${FIXTURES}/hello.cpp")
    file(COPY "${FIXTURES}/hello.rs" DESTINATION "${CMAKE_CURRENT_BINARY_DIR}/read-only"
        FILE_PERMISSIONS OWNER_READ GROUP_READ WORLD_READ)
    ferny_smoke(ui_status_read_only "smoke: status .*[|]Rust[|]Read-only"
        "${CMAKE_CURRENT_BINARY_DIR}/read-only/hello.rs")
    ferny_smoke(ui_documents_sidebar "smoke: documents hello.cpp[|]hello.rs"
        "${FIXTURES}/hello.cpp" "${FIXTURES}/hello.rs")
    ferny_smoke(ui_files_sidebar "smoke: browse ok sub[|]Alpha.md[|]b.txt"
        --smoke-browse "${PROJECT_SOURCE_DIR}/test/fixtures/browse")
    ferny_smoke(ui_files_sidebar_missing "smoke: browse failed [(]cannot list this folder[)]"
        --smoke-browse "${PROJECT_SOURCE_DIR}/test/fixtures/browse/missing")

    # Single instance: a second invocation forwards its files (Unix sockets).
    if(UNIX)
        add_test(NAME ui_ipc_forward
            COMMAND "${PROJECT_SOURCE_DIR}/test/ui/ipc_smoke.sh" $<TARGET_FILE:ferny> "${FIXTURES}")
    endif()

    # Editing (H1.3b): Trojan Source warning, comment toggling round trip.
    ferny_smoke(ui_smoke_bidi_warning "filetype=c .* bidi=6" "${FIXTURES}/trojan-source.c")
    ferny_smoke(ui_smoke_comment_toggle "smoke: save status=ok"
        "${FIXTURES}/hello.cpp" --smoke-command edit.select_all
        --smoke-command edit.toggle_comment --smoke-save-as "${ROUNDTRIP}/commented.cpp")
    set_tests_properties(ui_smoke_comment_toggle PROPERTIES FIXTURES_SETUP comment_toggle)
    add_test(NAME ui_compare_comment_toggle COMMAND ${CMAKE_COMMAND} -E compare_files
        "${FIXTURES}/hello.commented.cpp" "${ROUNDTRIP}/commented.cpp")
    set_tests_properties(ui_compare_comment_toggle PROPERTIES FIXTURES_REQUIRED comment_toggle)
    ferny_smoke(ui_smoke_uncomment "smoke: save status=ok"
        "${FIXTURES}/hello.commented.cpp" --smoke-command edit.select_all
        --smoke-command edit.toggle_comment --smoke-save-as "${ROUNDTRIP}/uncommented.cpp")
    set_tests_properties(ui_smoke_uncomment PROPERTIES FIXTURES_SETUP uncomment)
    add_test(NAME ui_compare_uncomment COMMAND ${CMAKE_COMMAND} -E compare_files
        "${FIXTURES}/hello.cpp" "${ROUNDTRIP}/uncommented.cpp")
    set_tests_properties(ui_compare_uncomment PROPERTIES FIXTURES_REQUIRED uncomment)
    # Typing: auto-close, type-over, Enter between braces, auto-indent.
    ferny_smoke(ui_smoke_typing "smoke: save status=ok"
        "${FIXTURES}/empty.c" "--smoke-type=int f(x) {\\nreturn [1] + g(2)\\n"
        --smoke-save-as "${ROUNDTRIP}/typed.c")
    set_tests_properties(ui_smoke_typing PROPERTIES FIXTURES_SETUP typing)
    add_test(NAME ui_compare_typing COMMAND ${CMAKE_COMMAND} -E compare_files
        "${FIXTURES}/typed.c" "${ROUNDTRIP}/typed.c")
    set_tests_properties(ui_compare_typing PROPERTIES FIXTURES_REQUIRED typing)
    # .editorconfig (H1.3c): indentation, save transforms on a copy; and
    # Geany's `-*- name -*-` modeline.
    set(EC_COPY "${CMAKE_CURRENT_BINARY_DIR}/editorconfig-test")
    add_test(NAME ui_editorconfig_copy COMMAND ${CMAKE_COMMAND} -E copy_directory
        "${PROJECT_SOURCE_DIR}/test/fixtures/editorconfig" "${EC_COPY}")
    set_tests_properties(ui_editorconfig_copy PROPERTIES FIXTURES_SETUP ec_copy)
    ferny_smoke(ui_editorconfig_indent "filetype=python .* indent=2" "${EC_COPY}/two.py")
    set_tests_properties(ui_editorconfig_indent PROPERTIES FIXTURES_REQUIRED ec_copy)
    ferny_smoke(ui_editorconfig_save "smoke: ok" "${EC_COPY}/messy.txt" --smoke-command file.save)
    set_tests_properties(ui_editorconfig_save PROPERTIES
        FIXTURES_REQUIRED ec_copy FIXTURES_SETUP ec_saved)
    add_test(NAME ui_editorconfig_compare COMMAND ${CMAKE_COMMAND} -E compare_files
        "${PROJECT_SOURCE_DIR}/test/fixtures/editorconfig-expected/messy.txt" "${EC_COPY}/messy.txt")
    set_tests_properties(ui_editorconfig_compare PROPERTIES FIXTURES_REQUIRED ec_saved)
    ferny_smoke(ui_smoke_modeline "filetype=python" "${FIXTURES}/modeline-notes")

    # Search (H1.4): find next/previous, regex replace all, find in files.
    ferny_smoke(ui_search_find_next "selection=92-100"
        "${FIXTURES}/hello.cpp" --smoke-find greeting --smoke-command search.find_next)
    ferny_smoke(ui_search_find_previous "selection=147-155"
        "${FIXTURES}/hello.cpp" --smoke-find greeting --smoke-command search.find_previous)
    ferny_smoke(ui_search_invalid_regex "selection=0-0"
        "${FIXTURES}/hello.cpp" --smoke-find "(unclosed" --smoke-flags 1
        --smoke-command search.find_next)
    ferny_smoke(ui_search_replace_all "smoke: save status=ok"
        "${FIXTURES}/hello.cpp" --smoke-find "printf[(]\"([^\"]*)\"" --smoke-flags 3
        --smoke-replace "puts(\"$1\"" --smoke-command search.replace_all
        --smoke-save-as "${ROUNDTRIP}/replaced.cpp")
    set_tests_properties(ui_search_replace_all PROPERTIES FIXTURES_SETUP replaced)
    add_test(NAME ui_compare_replace_all COMMAND ${CMAKE_COMMAND} -E compare_files
        "${FIXTURES}/hello.replaced.cpp" "${ROUNDTRIP}/replaced.cpp")
    set_tests_properties(ui_compare_replace_all PROPERTIES FIXTURES_REQUIRED replaced)
    ferny_smoke(ui_search_find_in_files "smoke: fif hits=2 truncated=no searched=2 skipped=1"
        --smoke-find needle --smoke-find-in-files "${PROJECT_SOURCE_DIR}/test/fixtures/search")

    # Settings and keybindings (H1.5a), from a test configuration directory.
    set(CONFIG_FIXTURES "${PROJECT_SOURCE_DIR}/test/fixtures/config")
    ferny_smoke(ui_settings_applied "indent=3 .* margin=0 edge=100"
        --config-dir "${CONFIG_FIXTURES}/custom" "${FIXTURES}/../config/plain.txt")
    ferny_smoke(ui_settings_invalid_use_defaults "indent=4 .* margin=48 edge=72"
        --config-dir "${CONFIG_FIXTURES}/broken" "${FIXTURES}/../config/plain.txt")
    ferny_smoke(ui_keybinding_override "smoke: shortcut Ctrl[+]Shift[+]F12"
        --config-dir "${CONFIG_FIXTURES}/custom" --smoke-shortcut file.save)
    ferny_smoke(ui_keybinding_default "smoke: shortcut Ctrl[+]S"
        --smoke-shortcut file.save)

    # Preferences categories and apply round trip (H5.2): every descriptor
    # exactly once, approved category layout, unchanged values accepted.
    # English forced so the expected category titles are deterministic.
    set(PREFS_CONFIG "${CMAKE_CURRENT_BINARY_DIR}/prefs-config")
    file(MAKE_DIRECTORY "${PREFS_CONFIG}")
    ferny_smoke(ui_prefs_categories "smoke: prefs coverage=ok rejected=0"
        --smoke-prefs --language en --config-dir "${PREFS_CONFIG}")

    # Symbols (H2.2): tree-sitter definitions in the Symbols sidebar.
    ferny_smoke(ui_symbols_cpp "smoke: symbols function:main:4" "${FIXTURES}/hello.cpp")
    ferny_smoke(ui_symbols_rust "smoke: symbols function:main:2" "${FIXTURES}/hello.rs")
    ferny_smoke(ui_symbols_plain "smoke: symbols \n" "${FIXTURES}/latin1.txt")

    # Navigation, completion and palette (H2.3), through the commands.
    set(SYMBOLS_RS "${FIXTURES}/symbols.rs")
    ferny_smoke(ui_goto_definition "cursor=2:1 " "${SYMBOLS_RS}:7:13" --smoke-command search.goto_definition)
    ferny_smoke(ui_navigate_back "cursor=7:13 " "${SYMBOLS_RS}:7:13"
        --smoke-command search.goto_definition --smoke-command view.navigate_back)
    ferny_smoke(ui_navigate_forward "cursor=2:1 " "${SYMBOLS_RS}:7:13"
        --smoke-command search.goto_definition --smoke-command view.navigate_back
        --smoke-command view.navigate_forward)
    ferny_smoke(ui_goto_symbol "cursor=6:1 " "${SYMBOLS_RS}" --smoke-input main
        --smoke-command search.goto_symbol)
    ferny_smoke(ui_complete_word "length=128 .*cursor=8:11 " "${SYMBOLS_RS}:8:5" --smoke-type hel
        --smoke-command edit.complete_word)
    ferny_smoke(ui_command_palette "cursor=2:1 " "${SYMBOLS_RS}:7:13" --smoke-input definition
        --smoke-command app.command_palette)

    # Build (H2.4): copies in the build tree (the commands write next to
    # the file). The C file name holds shell syntax that must stay inert.
    set(BUILD_TESTS "${CMAKE_CURRENT_BINARY_DIR}/build-tests")
    set(HOSTILE_C "${BUILD_TESTS}/a b $(echo pwned) 'q'.c")
    file(MAKE_DIRECTORY "${BUILD_TESTS}")
    file(COPY_FILE "${PROJECT_SOURCE_DIR}/test/fixtures/build/hello.c" "${HOSTILE_C}")
    file(COPY_FILE "${PROJECT_SOURCE_DIR}/test/fixtures/build/broken.c" "${BUILD_TESTS}/broken.c")
    file(COPY_FILE "${PROJECT_SOURCE_DIR}/test/fixtures/build/flood.c" "${BUILD_TESTS}/flood.c")
    file(COPY_FILE "${PROJECT_SOURCE_DIR}/test/fixtures/build/Makefile" "${BUILD_TESTS}/Makefile")
    # Untrusted folder: Build runs (R1), Execute of ./program does not (a
    # program may be planted there; phase-2 audit F1).
    ferny_smoke(ui_build_untrusted_execute_blocked
        "smoke: build build exit=0.*smoke: build execute not started"
        "${HOSTILE_C}" --smoke-build build,execute)
    set_tests_properties(ui_build_untrusted_execute_blocked PROPERTIES
        FAIL_REGULAR_EXPRESSION "compiler pwned")
    file(MAKE_DIRECTORY "${BUILD_TESTS}/planted")
    file(COPY_FILE "${PROJECT_SOURCE_DIR}/test/fixtures/build/hello.c" "${BUILD_TESTS}/planted/hello.c")
    file(WRITE "${BUILD_TESTS}/planted/hello" "#!/bin/sh\necho PLANTED-PROGRAM-RAN\n")
    file(CHMOD "${BUILD_TESTS}/planted/hello" PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE)
    ferny_smoke(ui_execute_planted_blocked "smoke: build execute not started"
        "${BUILD_TESTS}/planted/hello.c" --smoke-build execute)
    set_tests_properties(ui_execute_planted_blocked PROPERTIES
        FAIL_REGULAR_EXPRESSION "PLANTED-PROGRAM-RAN")
    ferny_smoke(ui_build_error_location "smoke: compiler broken.c:3:12: error"
        "${BUILD_TESTS}/broken.c" --smoke-build compile)

    ferny_smoke(ui_make_blocked_untrusted "smoke: build make not started"
        "${BUILD_TESTS}/broken.c" --smoke-build make)
    set(BUILD_TRUST "${CMAKE_CURRENT_BINARY_DIR}/build-trust-config")
    add_test(NAME ui_build_trust_clean COMMAND ${CMAKE_COMMAND} -E rm -rf "${BUILD_TRUST}")
    set_tests_properties(ui_build_trust_clean PROPERTIES FIXTURES_SETUP build_trust_clean)
    ferny_smoke(ui_build_trust "smoke: trust trusted" --config-dir "${BUILD_TRUST}"
        --smoke-trust "${BUILD_TESTS}")
    set_tests_properties(ui_build_trust PROPERTIES
        FIXTURES_REQUIRED build_trust_clean FIXTURES_SETUP build_trusted)
    ferny_smoke(ui_make_trusted "smoke: build make exit=0.*smoke: compiler made"
        --config-dir "${BUILD_TRUST}" "${BUILD_TESTS}/broken.c" --smoke-build make)
    set_tests_properties(ui_make_trusted PROPERTIES FIXTURES_REQUIRED build_trusted)
    # Trusted folder: build and execute; the shell syntax in the name stays
    # inert; the output is bounded.
    ferny_smoke(ui_build_and_execute_trusted
        "smoke: build build exit=0.*smoke: build execute exit=3.*smoke: compiler hello build"
        --config-dir "${BUILD_TRUST}" "${HOSTILE_C}" --smoke-build build,execute)
    set_tests_properties(ui_build_and_execute_trusted PROPERTIES
        FIXTURES_REQUIRED build_trusted FAIL_REGULAR_EXPRESSION "compiler pwned")
    ferny_smoke(ui_build_output_is_bounded "smoke: compiler [(]output truncated"
        --config-dir "${BUILD_TRUST}" "${BUILD_TESTS}/flood.c" --smoke-build build,execute)
    set_tests_properties(ui_build_output_is_bounded PROPERTIES FIXTURES_REQUIRED build_trusted)

    # Projects (H2.5a): a copy of the fixture project (its commands write
    # there), opened through the menu, reopened at start with its
    # indentation, commands blocked until trusted; a Geany project import.
    set(PROJ_DIR "${CMAKE_CURRENT_BINARY_DIR}/project-tests")
    set(PROJ "${PROJ_DIR}/proj")
    add_test(NAME ui_project_clean COMMAND ${CMAKE_COMMAND} -E rm -rf "${PROJ_DIR}")
    set_tests_properties(ui_project_clean PROPERTIES FIXTURES_SETUP project_clean)
    add_test(NAME ui_project_setup COMMAND ${CMAKE_COMMAND} -E copy_directory
        "${PROJECT_SOURCE_DIR}/test/fixtures/project" "${PROJ}")
    add_test(NAME ui_project_setup_geany COMMAND ${CMAKE_COMMAND} -E copy
        "${PROJECT_SOURCE_DIR}/test/fixtures/geany-project.geany" "${PROJ_DIR}/geany/demo.geany")
    set_tests_properties(ui_project_setup PROPERTIES
        FIXTURES_REQUIRED project_clean FIXTURES_SETUP project_copy)
    set_tests_properties(ui_project_setup_geany PROPERTIES
        FIXTURES_REQUIRED project_copy FIXTURES_SETUP project_geany)
    set(PROJ_ARGS --config-dir "${PROJ_DIR}/cfg" --session-file "${PROJ_DIR}/session.toml")
    ferny_smoke(ui_project_open "smoke: project Demo Project patterns=[*][.]c,[*][.]h.*session saved"
        ${PROJ_ARGS} --smoke-input "${PROJ}" --smoke-command project.open)
    set_tests_properties(ui_project_open PROPERTIES
        FIXTURES_REQUIRED project_copy FIXTURES_SETUP project_opened)
    ferny_smoke(ui_project_reopened "indent=3 .*smoke: project Demo Project"
        ${PROJ_ARGS} "${PROJ}/src/main.c")
    set_tests_properties(ui_project_reopened PROPERTIES FIXTURES_REQUIRED project_opened)
    ferny_smoke(ui_project_commands_need_trust "smoke: build compile not started"
        ${PROJ_ARGS} "${PROJ}/src/main.c" --smoke-build compile)
    set_tests_properties(ui_project_commands_need_trust PROPERTIES
        FIXTURES_REQUIRED project_opened FIXTURES_SETUP project_blocked)
    ferny_smoke(ui_project_trust "smoke: trust trusted" --config-dir "${PROJ_DIR}/cfg"
        --smoke-trust "${PROJ}")
    set_tests_properties(ui_project_trust PROPERTIES
        FIXTURES_REQUIRED project_blocked FIXTURES_SETUP project_trusted)
    ferny_smoke(ui_project_command_runs "smoke: build compile exit=0.*-o .*proj/out[.]o"
        ${PROJ_ARGS} "${PROJ}/src/main.c" --smoke-build compile)
    set_tests_properties(ui_project_command_runs PROPERTIES FIXTURES_REQUIRED project_trusted)
    ferny_smoke(ui_geany_project_import "smoke: project Imported Demo patterns=[*][.]c,[*][.]h"
        --config-dir "${PROJ_DIR}/cfg-geany" --smoke-input "${PROJ_DIR}/geany/demo.geany"
        --smoke-command project.import_geany)
    set_tests_properties(ui_geany_project_import PROPERTIES FIXTURES_REQUIRED project_geany)

    # Templates and snippets (H2.5b): Geany's main.c template (commented
    # header, C filetype) and the C "for" snippet with Tab.
    ferny_smoke(ui_new_from_template "smoke: current file= filetype=c length=837 "
        --smoke-input main.c --smoke-command file.new_from_template)
    ferny_smoke(ui_snippet_tab "length=67 .*cursor=2:21 " "${FIXTURES}/../templates/snippet.c:2:5"
        --smoke-type "for\\t")
    ferny_smoke(ui_tab_without_snippet "length=28 .*cursor=2:9 " "${FIXTURES}/../templates/snippet.c:2:5"
        --smoke-type "\\t")

    # Language servers (H3.1a): none for untrusted folders; with clangd
    # installed, real diagnostics in a trusted folder and secret-looking
    # files never sent (not verified where clangd is missing).
    ferny_smoke(ui_lsp_untrusted_no_server "smoke: lsp status=2 "
        "${BUILD_TESTS}/broken.c" --smoke-lsp-wait)
    find_program(CLANGD clangd)
    if(CLANGD)
        set(LSP_TESTS "${CMAKE_CURRENT_BINARY_DIR}/lsp-tests")
        file(MAKE_DIRECTORY "${LSP_TESTS}/work")
        file(COPY_FILE "${PROJECT_SOURCE_DIR}/test/fixtures/build/broken.c" "${LSP_TESTS}/work/broken.c")
        file(COPY_FILE "${PROJECT_SOURCE_DIR}/test/fixtures/build/broken.c" "${LSP_TESTS}/work/id_rsa.c")
        file(WRITE "${LSP_TESTS}/work/.editorconfig" "root = true\n")
        add_test(NAME ui_lsp_clean COMMAND ${CMAKE_COMMAND} -E rm -rf "${LSP_TESTS}/cfg")
        set_tests_properties(ui_lsp_clean PROPERTIES FIXTURES_SETUP lsp_clean)
        ferny_smoke(ui_lsp_trust "smoke: trust trusted" --config-dir "${LSP_TESTS}/cfg"
            --smoke-trust "${LSP_TESTS}/work")
        set_tests_properties(ui_lsp_trust PROPERTIES
            FIXTURES_REQUIRED lsp_clean FIXTURES_SETUP lsp_trusted)
        ferny_smoke(ui_lsp_diagnostics "smoke: lsp status=0 diagnostics=1:3"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_TESTS}/work/broken.c" --smoke-lsp-wait)
        ferny_smoke(ui_lsp_secret_file_not_sent "smoke: lsp status=3 "
            --config-dir "${LSP_TESTS}/cfg" "${LSP_TESTS}/work/id_rsa.c" --smoke-lsp-wait)
        set_tests_properties(ui_lsp_diagnostics ui_lsp_secret_file_not_sent PROPERTIES
            FIXTURES_REQUIRED lsp_trusted)
        # Symbols from the server (H4.2a): the sidebar and the workspace.
        ferny_smoke(ui_lsp_symbols "smoke: symbols function:main:4.*smoke: symbols-source lsp"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_TESTS}/work/main.c" --smoke-wait 2000)
        ferny_smoke(ui_lsp_workspace_symbol "smoke: usages [1-9]"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_TESTS}/work/main.c" --smoke-input main
            --smoke-delay 2000 --smoke-command search.goto_workspace_symbol --smoke-wait 2000)
        set_tests_properties(ui_lsp_symbols ui_lsp_workspace_symbol PROPERTIES
            FIXTURES_REQUIRED lsp_trusted)
        # Server features (H3.1b) on a header the editor never opened.
        file(COPY "${PROJECT_SOURCE_DIR}/test/fixtures/lsp/main.c"
            "${PROJECT_SOURCE_DIR}/test/fixtures/lsp/helper.h" DESTINATION "${LSP_TESTS}/work")
        set(LSP_MAIN "${LSP_TESTS}/work/main.c")
        ferny_smoke(ui_lsp_definition "cursor=6:5 .*smoke: current file=helper[.]h "
            --config-dir "${LSP_TESTS}/cfg" "${LSP_MAIN}:5:14"
            --smoke-command search.goto_definition --smoke-wait 2000)
        ferny_smoke(ui_lsp_usages "smoke: usages [2-9]"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_MAIN}:5:14"
            --smoke-command search.find_usages --smoke-wait 2000)
        ferny_smoke(ui_lsp_hover "hover=function helper"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_MAIN}:5:14" --smoke-hover --smoke-wait 2000)
        ferny_smoke(ui_lsp_signature "calltip=yes"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_MAIN}:6:5" --smoke-type "helper(" --smoke-wait 2000)
        ferny_smoke(ui_lsp_completion "autoc=yes"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_MAIN}:6:5" --smoke-type "hel"
            --smoke-command edit.complete_word --smoke-wait 2000)
        # Rename and formatting (H3.1c): applied in the editor, unsaved.
        file(COPY "${PROJECT_SOURCE_DIR}/test/fixtures/lsp/ugly.c" DESTINATION "${LSP_TESTS}/work")
        ferny_smoke(ui_lsp_rename "smoke: current file=main[.]c filetype=c length=143 modified=yes"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_MAIN}:5:14" --smoke-input add_one
            --smoke-command search.rename_symbol --smoke-wait 2000)
        ferny_smoke(ui_lsp_format "smoke: current file=ugly[.]c filetype=c length=4[0-9] modified=yes"
            --config-dir "${LSP_TESTS}/cfg" "${LSP_TESTS}/work/ugly.c"
            --smoke-command document.format --smoke-wait 2000)
        set_tests_properties(ui_lsp_definition ui_lsp_usages ui_lsp_hover ui_lsp_signature
            ui_lsp_completion ui_lsp_rename ui_lsp_format PROPERTIES FIXTURES_REQUIRED lsp_trusted)
    endif()

    # Workspace trust (H2.1): restricted by default, trusted after Document
    # > Trust Folder... (command path), never the root.
    ferny_smoke(ui_trust_default_restricted "smoke: status .*[|]Restricted" "${FIXTURES}/hello.cpp")
    set(TRUST_CONFIG "${CMAKE_CURRENT_BINARY_DIR}/trust-config")
    add_test(NAME ui_trust_clean COMMAND ${CMAKE_COMMAND} -E rm -rf "${TRUST_CONFIG}")
    set_tests_properties(ui_trust_clean PROPERTIES FIXTURES_SETUP trust_clean)
    ferny_smoke(ui_trust_folder "smoke: trust trusted folders=1"
        --config-dir "${TRUST_CONFIG}" --smoke-trust "${FIXTURES}")
    set_tests_properties(ui_trust_folder PROPERTIES
        FIXTURES_REQUIRED trust_clean FIXTURES_SETUP trust_saved)
    ferny_smoke(ui_trust_applies "smoke: status .*[|]Trusted"
        --config-dir "${TRUST_CONFIG}" "${FIXTURES}/hello.cpp")
    set_tests_properties(ui_trust_applies PROPERTIES FIXTURES_REQUIRED trust_saved)
    ferny_smoke(ui_trust_root_refused "smoke: trust refused folders=0" --smoke-trust /)

    # Translations (H1.6b): embedded Spanish catalogue, by option and by
    # setting; English by default in smoke tests and for unknown values.
    ferny_smoke(ui_language_es
        "smoke: menus &Archivo[|]&Editar[|]&Buscar[|]&Ver[|]&Documento[|]&Proyecto[|]&Construir[|]Ay&uda.*smoke: status Línea 1/9, col. 1[|]Sel. 0[|]Espacios: 4"
        --language es "${FIXTURES}/hello.cpp")
    ferny_smoke(ui_language_setting_es "smoke: menus &Archivo[|]"
        --config-dir "${CONFIG_FIXTURES}/spanish")
    ferny_smoke(ui_language_default_en "smoke: menus &File[|]&Edit[|]&Search[|]&View[|]&Document[|]&Project[|]&Build[|]&Help")
    ferny_smoke(ui_language_unknown_en "smoke: menus &File[|]" --language xx)

    # Geany importer (H1.5b): import into a fresh configuration directory,
    # then start with it.
    set(GEANY_OUT "${CMAKE_CURRENT_BINARY_DIR}/geany-import-config")
    add_test(NAME ui_geany_clean COMMAND ${CMAKE_COMMAND} -E rm -rf "${GEANY_OUT}")
    set_tests_properties(ui_geany_clean PROPERTIES FIXTURES_SETUP geany_clean)
    ferny_smoke(ui_geany_import "smoke: geany settings=15 keybindings=5 files=3 not_imported=9 rejected=0 saved=yes"
        --config-dir "${GEANY_OUT}" --smoke-import-geany "${PROJECT_SOURCE_DIR}/test/fixtures/geany-config")
    set_tests_properties(ui_geany_import PROPERTIES
        FIXTURES_REQUIRED geany_clean FIXTURES_SETUP geany_imported)
    ferny_smoke(ui_geany_imported_filetype "lexer=cpp .* filetype=foo scheme=my_theme back=101010"
        --config-dir "${GEANY_OUT}" "${PROJECT_SOURCE_DIR}/test/fixtures/geany-config/sample.foo")
    set_tests_properties(ui_geany_imported_filetype PROPERTIES FIXTURES_REQUIRED geany_imported)
    ferny_smoke(ui_geany_imported_shortcut "smoke: shortcut Ctrl[+]Shift[+]S"
        --config-dir "${GEANY_OUT}" --smoke-shortcut file.save)
    set_tests_properties(ui_geany_imported_shortcut PROPERTIES FIXTURES_REQUIRED geany_imported)

    # Plugins (H3.2): one runs and logs, a hostile one is stopped, safe
    # mode loads none.
    set(PLUGIN_CFG "${CMAKE_CURRENT_BINARY_DIR}/plugin-tests/cfg")
    add_test(NAME ui_plugins_clean COMMAND ${CMAKE_COMMAND} -E rm -rf
        "${CMAKE_CURRENT_BINARY_DIR}/plugin-tests")
    set_tests_properties(ui_plugins_clean PROPERTIES FIXTURES_SETUP plugins_clean)
    add_test(NAME ui_plugins_setup COMMAND ${CMAKE_COMMAND} -E copy_directory
        "${PROJECT_SOURCE_DIR}/test/fixtures/plugins" "${PLUGIN_CFG}/plugins")
    set_tests_properties(ui_plugins_setup PROPERTIES
        FIXTURES_REQUIRED plugins_clean FIXTURES_SETUP plugins_copy)
    ferny_smoke(ui_plugins_run "smoke: plugins ferny-classbuilder=running,ferny-htmlchars=running,addon-addons=running,addon-geniuspaste=running,addon-lipsum=running,addon-macros=running,evil=failed,hello=running,sleepy=running,tools=running"
        --config-dir "${PLUGIN_CFG}" --smoke-grant-plugins)
    set_tests_properties(ui_plugins_run PROPERTIES
        FIXTURES_REQUIRED plugins_copy FIXTURES_SETUP plugins_ran)
    add_test(NAME ui_plugins_log COMMAND ${CMAKE_COMMAND} -E cat "${PLUGIN_CFG}/ferny.log")
    set_tests_properties(ui_plugins_log PROPERTIES FIXTURES_REQUIRED plugins_ran
        PASS_REGULAR_EXPRESSION "plugin hello: hello from plugin, clipboard=true")
    ferny_smoke(ui_plugins_safe_mode "smoke: plugins ferny-classbuilder=safe-mode,ferny-htmlchars=safe-mode,addon-addons=safe-mode,addon-geniuspaste=safe-mode,addon-lipsum=safe-mode,addon-macros=safe-mode,evil=safe-mode,.*tools=safe-mode"
        --config-dir "${PLUGIN_CFG}" --safe-mode)
    set_tests_properties(ui_plugins_safe_mode PROPERTIES FIXTURES_REQUIRED plugins_ran)
    # Plugin API (H3.3a): an event handler sets the status, a handler that
    # never returns is disabled, a command edits the document (unsaved) in
    # the plugin's translation and appears under Tools.
    ferny_smoke(ui_plugins_event
        "smoke: plugins ferny-classbuilder=running,ferny-htmlchars=running,addon-addons=running,addon-geniuspaste=running,addon-lipsum=running,addon-macros=running,evil=failed,hello=running,sleepy=failed,tools=running.*smoke: message hello: opened cpp"
        --config-dir "${PLUGIN_CFG}" "${FIXTURES}/hello.cpp")
    ferny_smoke(ui_plugins_command
        "length=184 modified=yes.*smoke: message stamped.*smoke: menus .*[|]&Herramientas[|]"
        --config-dir "${PLUGIN_CFG}" --language es "${FIXTURES}/hello.cpp"
        --smoke-command plugin.hello.stamp)
    set_tests_properties(ui_plugins_event ui_plugins_command PROPERTIES
        FIXTURES_REQUIRED plugins_ran)
    # Permission-gated APIs (H3.3b): in a trusted project, a plugin lists,
    # writes a file and runs a program whose result reaches its callback.
    set(PLUGIN_PROJ "${CMAKE_CURRENT_BINARY_DIR}/plugin-tests/proj")
    add_test(NAME ui_plugins_project_setup COMMAND ${CMAKE_COMMAND} -E copy_directory
        "${PROJECT_SOURCE_DIR}/test/fixtures/project" "${PLUGIN_PROJ}")
    set_tests_properties(ui_plugins_project_setup PROPERTIES
        FIXTURES_REQUIRED plugins_ran FIXTURES_SETUP plugins_project)
    ferny_smoke(ui_plugins_project_trust "smoke: trust trusted"
        --config-dir "${PLUGIN_CFG}" --smoke-trust "${PLUGIN_PROJ}")
    set_tests_properties(ui_plugins_project_trust PROPERTIES
        FIXTURES_REQUIRED plugins_project FIXTURES_SETUP plugins_trusted)
    ferny_smoke(ui_plugins_project_apis "smoke: message exit 0 spawned"
        --config-dir "${PLUGIN_CFG}" --session-file "${PLUGIN_CFG}/session.toml"
        --smoke-input "${PLUGIN_PROJ}" --smoke-command project.open
        --smoke-command plugin.tools.go --smoke-wait 3000)
    set_tests_properties(ui_plugins_project_apis PROPERTIES
        FIXTURES_REQUIRED plugins_trusted FIXTURES_SETUP plugins_apis)
    # Extension points (H3.3c): completion, hover and formatting from a
    # plugin; before_save changes what is written.
    ferny_smoke(ui_plugins_completion "length=192 modified=yes"
        --config-dir "${PLUGIN_CFG}" "${FIXTURES}/hello.cpp" --smoke-type "zq"
        --smoke-command edit.complete_word)
    ferny_smoke(ui_plugins_hover "hover=plugin hover main"
        --config-dir "${PLUGIN_CFG}" "${FIXTURES}/hello.cpp:4:6" --smoke-hover)
    ferny_smoke(ui_plugins_format "length=191 modified=yes"
        --config-dir "${PLUGIN_CFG}" "${FIXTURES}/hello.cpp" --smoke-command document.format)
    set(PLUGIN_TODO "${CMAKE_CURRENT_BINARY_DIR}/plugin-tests/docs/todo.cpp")
    add_test(NAME ui_plugins_todo_setup COMMAND ${CMAKE_COMMAND} -E copy
        "${PROJECT_SOURCE_DIR}/test/fixtures/plugin-docs/todo.cpp" "${PLUGIN_TODO}")
    set_tests_properties(ui_plugins_todo_setup PROPERTIES
        FIXTURES_REQUIRED plugins_ran FIXTURES_SETUP plugins_todo)
    ferny_smoke(ui_plugins_before_save "smoke: current file=todo[.]cpp"
        --config-dir "${PLUGIN_CFG}" "${PLUGIN_TODO}" --smoke-command file.save)
    set_tests_properties(ui_plugins_before_save PROPERTIES
        FIXTURES_REQUIRED plugins_todo FIXTURES_SETUP plugins_saved)
    add_test(NAME ui_plugins_before_save_written COMMAND ${CMAKE_COMMAND} -E cat "${PLUGIN_TODO}")
    set_tests_properties(ui_plugins_before_save_written PROPERTIES
        FIXTURES_REQUIRED plugins_saved PASS_REGULAR_EXPRESSION "// DONE: synthetic")
    set_tests_properties(ui_plugins_completion ui_plugins_hover ui_plugins_format PROPERTIES
        FIXTURES_REQUIRED plugins_ran)
    # Bundled plugins (H3.4): HTML Characters encodes the selection; Class
    # Builder writes a class into the trusted project of the session.
    ferny_smoke(ui_plugins_htmlchars "length=190 modified=yes"
        --config-dir "${PLUGIN_CFG}" "${FIXTURES}/hello.cpp"
        --smoke-command edit.select_all --smoke-command plugin.ferny-htmlchars.encode)
    set_tests_properties(ui_plugins_htmlchars PROPERTIES FIXTURES_REQUIRED plugins_ran)
    file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/plugin-widget.txt" "Widget")
    ferny_smoke(ui_plugins_classbuilder "smoke: message Created Widget[.]h and Widget[.]cpp"
        --config-dir "${PLUGIN_CFG}" --session-file "${PLUGIN_CFG}/session.toml"
        "${CMAKE_CURRENT_BINARY_DIR}/plugin-widget.txt"
        --smoke-command edit.select_all --smoke-command plugin.ferny-classbuilder.cpp_class)
    set_tests_properties(ui_plugins_classbuilder PROPERTIES
        FIXTURES_REQUIRED plugins_apis FIXTURES_SETUP plugins_class)
    add_test(NAME ui_plugins_classbuilder_written COMMAND ${CMAKE_COMMAND} -E cat "${PLUGIN_PROJ}/Widget.h")
    set_tests_properties(ui_plugins_classbuilder_written PROPERTIES
        FIXTURES_REQUIRED plugins_class PASS_REGULAR_EXPRESSION "class Widget [{]")
    add_test(NAME ui_plugins_project_written
        COMMAND ${CMAKE_COMMAND} -E cat "${PLUGIN_PROJ}/plugin-out.txt")
    set_tests_properties(ui_plugins_project_written PROPERTIES
        FIXTURES_REQUIRED plugins_apis PASS_REGULAR_EXPRESSION "written by a plugin")

    ferny_smoke(ui_smoke_unknown_command "smoke: unknown command"
        "${FIXTURES}/hello.cpp" --smoke-command edit.format_disk)

    # Negative cases: the file is rejected and the editor stays empty.
    ferny_smoke(ui_smoke_reject_binary "smoke: failed status=binary length=0"
        "${FIXTURES}/binary.bin")
    ferny_smoke(ui_smoke_reject_missing "smoke: failed status=not-found length=0"
        "${FIXTURES}/does-not-exist.txt")
    ferny_smoke(ui_smoke_reject_directory "smoke: failed status=not-a-file length=0"
        "${FIXTURES}")

    # Save Actions (H4.2a): autosave writes the modified file; the version
    # it replaced is kept under the configuration's backups folder.
    set(SAVE_TESTS "${CMAKE_CURRENT_BINARY_DIR}/save-tests")
    file(WRITE "${SAVE_TESTS}/cfg/settings.toml" "[files]\nautosave = true\nbackup_copies = true\n")
    file(WRITE "${SAVE_TESTS}/check_backup.cmake"
        "file(GLOB_RECURSE copies \"${SAVE_TESTS}/cfg/backups/*-a.txt\")\n"
        "list(LENGTH copies n)\n"
        "if(NOT n EQUAL 1)\n  message(FATAL_ERROR \"expected 1 backup, found \${n}\")\nendif()\n"
        "file(READ \"\${copies}\" old)\n"
        "if(NOT old STREQUAL \"before\\n\")\n  message(FATAL_ERROR \"backup holds the wrong version\")\nendif()\n")
    add_test(NAME ui_save_actions_setup COMMAND ${CMAKE_COMMAND} -E rm -rf
        "${SAVE_TESTS}/cfg/backups" "${SAVE_TESTS}/a.txt")
    add_test(NAME ui_save_actions_file COMMAND ${CMAKE_COMMAND} -E copy
        "${PROJECT_SOURCE_DIR}/test/fixtures/plugin-docs/before.txt" "${SAVE_TESTS}/a.txt")
    set_tests_properties(ui_save_actions_setup PROPERTIES FIXTURES_SETUP save_clean)
    set_tests_properties(ui_save_actions_file PROPERTIES
        FIXTURES_REQUIRED save_clean FIXTURES_SETUP save_file)
    ferny_smoke(ui_save_actions_autosave "smoke: autosaved 1"
        --config-dir "${SAVE_TESTS}/cfg" "${SAVE_TESTS}/a.txt" --smoke-type "x" --smoke-autosave)
    set_tests_properties(ui_save_actions_autosave PROPERTIES
        FIXTURES_REQUIRED save_file FIXTURES_SETUP save_done)
    add_test(NAME ui_save_actions_written COMMAND ${CMAKE_COMMAND} -E cat "${SAVE_TESTS}/a.txt")
    set_tests_properties(ui_save_actions_written PROPERTIES FIXTURES_REQUIRED save_done
        PASS_REGULAR_EXPRESSION "^xbefore")
    add_test(NAME ui_save_actions_backup COMMAND ${CMAKE_COMMAND} -P "${SAVE_TESTS}/check_backup.cmake")
    set_tests_properties(ui_save_actions_backup PROPERTIES FIXTURES_REQUIRED save_done)
    # Off by default: nothing is saved without the setting.
    ferny_smoke(ui_save_actions_off "smoke: autosaved 0"
        "${FIXTURES}/hello.cpp" --smoke-type "x" --smoke-autosave)

    # Add-ons (H4.2a): mark word, enclose the selection.
    ferny_smoke(ui_mark_occurrences "smoke: message 2 occurrence[(]s[)] marked"
        "${FIXTURES}/hello.cpp:5:17" --smoke-command edit.mark_occurrences)
    ferny_smoke(ui_enclose_selection "length=180 modified=yes"
        "${FIXTURES}/hello.cpp" --smoke-select-all --smoke-type "(")

    # Git read-only (H4.2d): a repository built at test time; status marks
    # and blame in a trusted folder, nothing in an untrusted one.
    find_program(GIT git)
    if(GIT)
        set(GIT_TESTS "${CMAKE_CURRENT_BINARY_DIR}/git-tests")
        set(GIT_REPO "${GIT_TESTS}/repo")
        set(GIT_RUN ${CMAKE_COMMAND} -E env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 "${GIT}")
        file(WRITE "${GIT_TESTS}/make_repo.cmake"
            "file(REMOVE_RECURSE \"${GIT_TESTS}/repo\" \"${GIT_TESTS}/cfg\" \"${GIT_TESTS}/other\")\n"
            "file(MAKE_DIRECTORY \"${GIT_REPO}/sub\")\n"
            "file(WRITE \"${GIT_REPO}/a.txt\" \"one\\n\")\n"
            "file(WRITE \"${GIT_REPO}/sub/b.txt\" \"two\\n\")\n"
            "execute_process(COMMAND ${CMAKE_COMMAND} -E env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \"${GIT}\" init -q \"${GIT_REPO}\" COMMAND_ERROR_IS_FATAL ANY)\n"
            "execute_process(COMMAND ${CMAKE_COMMAND} -E env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \"${GIT}\" -C \"${GIT_REPO}\" add a.txt sub/b.txt COMMAND_ERROR_IS_FATAL ANY)\n"
            "execute_process(COMMAND ${CMAKE_COMMAND} -E env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \"${GIT}\" -C \"${GIT_REPO}\" -c user.name=Test -c user.email=test@example.com -c core.hooksPath= -c commit.gpgsign=false commit -q -m \"First commit\" COMMAND_ERROR_IS_FATAL ANY)\n"
            "file(WRITE \"${GIT_REPO}/a.txt\" \"one\\nchanged\\n\")\n"
            "file(WRITE \"${GIT_REPO}/new.txt\" \"x\\n\")\n")
        add_test(NAME ui_git_setup COMMAND ${CMAKE_COMMAND} -P "${GIT_TESTS}/make_repo.cmake")
        set_tests_properties(ui_git_setup PROPERTIES FIXTURES_SETUP git_repo)
        ferny_smoke(ui_git_trust "smoke: trust trusted"
            --config-dir "${GIT_TESTS}/cfg" --smoke-trust "${GIT_REPO}")
        set_tests_properties(ui_git_trust PROPERTIES
            FIXTURES_REQUIRED git_repo FIXTURES_SETUP git_trusted)
        ferny_smoke(ui_git_status "smoke: git a[.]txt=M,new[.]txt=[?]\n"
            --config-dir "${GIT_TESTS}/cfg" --smoke-browse "${GIT_REPO}" --smoke-wait 3000)
        ferny_smoke(ui_git_blame "smoke: usages 2 "
            --config-dir "${GIT_TESTS}/cfg" "${GIT_REPO}/a.txt"
            --smoke-command document.git_blame --smoke-wait 3000)
        set_tests_properties(ui_git_status ui_git_blame PROPERTIES FIXTURES_REQUIRED git_trusted)
        ferny_smoke(ui_git_untrusted "smoke: git \n"
            --config-dir "${GIT_TESTS}/other" --smoke-browse "${GIT_REPO}" --smoke-wait 1500)
        set_tests_properties(ui_git_untrusted PROPERTIES FIXTURES_REQUIRED git_repo)
    endif()

    # Spell checking (H4.2c): a synthetic dictionary; plain text checked
    # whole, code only in comments and strings; off by default.
    set(SPELL_TESTS "${CMAKE_CURRENT_BINARY_DIR}/spell-tests")
    file(WRITE "${SPELL_TESTS}/cfg/settings.toml" "[spell]\nenabled = true\nlanguage = \"xx_TEST\"\n")
    file(COPY "${PROJECT_SOURCE_DIR}/test/fixtures/spell/xx_TEST.aff"
        "${PROJECT_SOURCE_DIR}/test/fixtures/spell/xx_TEST.dic"
        DESTINATION "${SPELL_TESTS}/cfg/dictionaries")
    ferny_smoke(ui_spell_text "smoke: spelling 2"
        --config-dir "${SPELL_TESTS}/cfg" "${PROJECT_SOURCE_DIR}/test/fixtures/spell/sample.txt")
    ferny_smoke(ui_spell_code "smoke: spelling 2"
        --config-dir "${SPELL_TESTS}/cfg" "${PROJECT_SOURCE_DIR}/test/fixtures/spell/sample.c")
    ferny_smoke(ui_spell_off "smoke: spelling 0"
        "${PROJECT_SOURCE_DIR}/test/fixtures/spell/sample.txt")
    ferny_smoke(ui_spell_toggle "smoke: spelling 0"
        --config-dir "${SPELL_TESTS}/cfg" "${PROJECT_SOURCE_DIR}/test/fixtures/spell/sample.txt"
        --smoke-command view.spell_check)

    # Markdown preview (H4.2b): raw HTML shown as text, images as alt text.
    ferny_smoke(ui_markdown_preview
        "smoke: preview Preview Title Some text with a safe link and a bad one[.] <script>alert[(]'x'[)]</script> [[]image: logo[]]"
        "${PROJECT_SOURCE_DIR}/test/fixtures/markdown/hostile.md" --smoke-command view.markdown_preview)
    ferny_smoke(ui_markdown_preview_other "smoke: preview The preview shows Markdown documents[.]"
        "${FIXTURES}/hello.cpp" --smoke-command view.markdown_preview)

    # Split view (H4.1b): the second view shares the current document;
    # closing the tab it shows moves it to the current one.
    ferny_smoke(ui_split_shared "smoke: split side hello[.]cpp shared=yes"
        "${FIXTURES}/hello.cpp" --smoke-command view.split_side)
    ferny_smoke(ui_split_stacked "smoke: split stacked hello[.]cpp shared=yes"
        "${FIXTURES}/hello.cpp" --smoke-command view.split_stacked)
    ferny_smoke(ui_split_unsplit "smoke: split none"
        "${FIXTURES}/hello.cpp" --smoke-command view.split_side --smoke-command view.unsplit)
    ferny_smoke(ui_split_close "smoke: current file=hello[.]cpp.*smoke: split side hello[.]cpp shared=yes"
        "${FIXTURES}/hello.cpp" "${FIXTURES}/hello.rs" --smoke-command view.split_side
        --smoke-command file.close)

    # Export (H4.1a): HTML and LaTeX escaped, PDF written.
    set(EXPORT_DIR "${CMAKE_CURRENT_BINARY_DIR}/export-tests")
    file(MAKE_DIRECTORY "${EXPORT_DIR}")
    foreach(format html tex pdf)
        ferny_smoke(ui_export_${format} "smoke: export ok"
            "${FIXTURES}/hello.cpp" --smoke-export "${EXPORT_DIR}/hello.${format}")
        set_tests_properties(ui_export_${format} PROPERTIES FIXTURES_SETUP export_${format})
    endforeach()
    add_test(NAME ui_export_html_content COMMAND ${CMAKE_COMMAND} -E cat "${EXPORT_DIR}/hello.html")
    set_tests_properties(ui_export_html_content PROPERTIES FIXTURES_REQUIRED export_html
        PASS_REGULAR_EXPRESSION "#include &lt;cstdio&gt;</span>.*<span class=\"s[0-9]+\">int</span>")
    add_test(NAME ui_export_tex_content COMMAND ${CMAKE_COMMAND} -E cat "${EXPORT_DIR}/hello.tex")
    set_tests_properties(ui_export_tex_content PROPERTIES FIXTURES_REQUIRED export_tex
        PASS_REGULAR_EXPRESSION "[\\]char37[{][}]s.*[\\]end[{]alltt[}]")
    add_test(NAME ui_export_pdf_content COMMAND ${CMAKE_COMMAND} -E cat "${EXPORT_DIR}/hello.pdf")
    set_tests_properties(ui_export_pdf_content PROPERTIES FIXTURES_REQUIRED export_pdf
        PASS_REGULAR_EXPRESSION "^%PDF-1[.][0-9]")

    # Round trips: open, save as, compare byte for byte with the original.
    foreach(fixture latin1.txt utf16le-bom.txt utf8-bom-crlf.txt hello.cpp)
        string(MAKE_C_IDENTIFIER "${fixture}" id)
        ferny_smoke(ui_roundtrip_save_${id} "smoke: save status=ok"
            "${FIXTURES}/${fixture}" --smoke-save-as "${ROUNDTRIP}/${fixture}")
        set_tests_properties(ui_roundtrip_save_${id} PROPERTIES FIXTURES_SETUP roundtrip_${id})
        add_test(NAME ui_roundtrip_compare_${id}
            COMMAND ${CMAKE_COMMAND} -E compare_files
                "${FIXTURES}/${fixture}" "${ROUNDTRIP}/${fixture}")
        set_tests_properties(ui_roundtrip_compare_${id} PROPERTIES
            FIXTURES_REQUIRED roundtrip_${id})
    endforeach()
endif()
