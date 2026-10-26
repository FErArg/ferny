# Security regression tests

Security tests are kept separate from functional tests. Each control in
`docs/THREAT-MODEL.md` (T1…T17) gets at least one test named after it, e.g.
`workspace_trust_blocks_build_t1`. Every fixed vulnerability keeps a
regression test.

Minimum surfaces for Ferny:

- workspace trust: build, LSP and project plugins blocked in untrusted folders;
- process spawning: file names with shell metacharacters, newlines and
  quotes are passed as single arguments;
- executable resolution ignores project-local directories;
- Lua sandbox: forbidden globals unavailable, permission denials, CPU and
  memory limits;
- IPC: socket permissions, oversized and malformed requests;
- save: atomicity, symlinks, read-only files, permission preservation;
- untrusted content: huge files, long lines, invalid encodings, bidi and
  invisible characters, ANSI escapes in build output;
- Geany importer: path traversal, oversized values, malformed keyfiles;
- logs contain no file contents.

Use only synthetic fixtures (`test/security/fixtures/`). Never real user
files, credentials or personal data.

## Index: threat → tests (phase-1 audit, 2026-10-16)

Security tests live next to the code they test (plan v3); this index maps
each threat of `docs/THREAT-MODEL.md` to them. `ct:` marks a CTest smoke
test in `ui/CMakeLists.txt`; `fuzz:` a cargo-fuzz target in `fuzz/`;
`sh:` a test in `test/scripts/`.

| Threat | Tests |
|---|---|
| T1–T3 gate | `ferny-core/tests/trust.rs`: all 7 tests; ct: `ui_trust_default_restricted`, `ui_trust_folder`, `ui_trust_applies`, `ui_trust_root_refused`, `ui_make_blocked_untrusted`, `ui_make_trusted` |
| T2, T3 build | `ferny-core/tests/git.rs`: `args_neutralise_repository_config`, `status_is_parsed`, `blame_is_parsed_and_neutralised`, `parsers_are_bounded`; ct: `ui_git_status`, `ui_git_blame`, `ui_git_untrusted`; `ferny-build/tests/build.rs`: `shell_syntax_is_rejected`, `placeholders_never_split_arguments`, `resolution_skips_project_and_relative_path`, `build_store_is_validated`; fuzz: `build_commands`; ct: `ui_build_and_execute_trusted` (hostile file name), `ui_build_untrusted_execute_blocked`, `ui_execute_planted_blocked` (phase-2 audit F1) |
| T1, T3, T11 projects | `ferny-config/tests/project.rs`: `base_path_is_confined`, `invalid_entries_are_dropped`, `invalid_files_are_refused`, `create_never_overwrites`, `geany_project_import_maps_and_lists`; fuzz: `project`; ct: `ui_project_commands_need_trust`, `ui_project_command_runs` |
| T1, T2, T6, T16 language servers | `ferny-lsp/tests/lsp.rs`: `framing_is_bounded`, `secret_files_are_not_synced`, `environment_is_reduced`, `uris_round_trip_and_reject_junk`, `diagnostics_are_parsed_and_bounded`, `positions_are_clamped`, `lifecycle_*`, `answers_are_parsed_and_bounded`, `edits_are_parsed_bounded_and_applied`; fuzz: `lsp`; ct: `ui_lsp_untrusted_no_server`, `ui_lsp_secret_file_not_sent` (with clangd) |
| T4, T5, T17 plugins | `ferny-plugin/tests/plugin.rs`: `sandbox_allows_the_safe_library_and_the_api`, `sandbox_blocks_escapes`, `limits_stop_loops_and_memory_bombs`, `manifests_are_validated`, `discovery_skips_links_and_bad_plugins`, `grants_are_tied_to_the_manifest`, `events_run_and_see_no_text`, `edits_only_in_commands`, `commands_are_validated`, `actions_are_bounded_and_texts_neutralised`, `slow_events_hit_the_time_limit`, `locale_is_bounded`, `fs_reads_inside_the_trusted_project_only`, `fs_read_any_needs_confirmation`, `fs_writes_are_confined`, `spawn_is_requested_bounded_and_delivered`, `processes_time_out_and_output_is_bounded`, `clipboard_reads_only_in_commands`, `confirmations_are_remembered_until_the_manifest_changes`, `providers_answer_for_their_filetypes`, `provider_answers_are_bounded`, `before_save_chains_and_hits_the_time_limit`, `stubs_match_the_api`, `bundled_plugins_load`, `classbuilder_writes_new_files_in_the_project_only`, `reserved_ids_are_refused`, `new_decisions_keep_answers_and_reset_clears_them`, `processes_and_their_children_end`; `ferny-plugin/tests/patterns.rs`: `results_match_lua`, `backtracking_hits_the_time_limit`; fuzz: `lua_patterns`; `ferny-config/tests/config.rs`: `plugin_keys_never_take_built_in_keys`; fuzz: `plugin_manifest`; ct: `ui_plugins_run` (hostile plugin stopped), `ui_plugins_log`, `ui_plugins_safe_mode`, `ui_plugins_event` (slow handler disabled), `ui_plugins_command`, `ui_plugins_project_apis`, `ui_plugins_project_written`, `ui_plugins_completion`, `ui_plugins_hover`, `ui_plugins_format`, `ui_plugins_before_save_written`, `ui_plugins_htmlchars`, `ui_plugins_classbuilder_written` |
| T12 | Not applicable yet (downloads: phase 4) |
| T6 Crafted file freezes or crashes the editor | `spell.rs`: `dictionaries_are_looked_up_in_order_and_validated`, `checks_are_bounded`, `personal_words_are_kept_private`; `ferny-core/tests/document.rs`: `bounded_read_accepts_exact_limit_and_rejects_more`, `large_flag_above_threshold`, `binary_file_is_refused`, `rejects_fifo_without_blocking`; `search.rs`: `pathological_pattern_is_linear`, `skips_large_and_binary`, `stops_at_result_limit`, `cancel_stops_the_walk`; `editorconfig.rs`: `oversized_editorconfig_is_ignored`, `pathological_glob_is_fast`; `browse.rs`: `listing_is_bounded`, `symlinks_are_not_followed`; `ferny-syntax/tests/symbols.rs`: `broken_and_hostile_input_is_bounded`, `many_definitions_are_capped`; fuzz: `decode`, `search`, `settings`, `geany_import`, `symbols` |
| T7 Deceptive content | `markdown.rs`: `raw_html_is_text`, `images_are_not_loaded`, `links_are_filtered`, `large_documents_are_not_previewed`; ct: `ui_markdown_preview`; `export.rs`: `html_escapes_everything`, `latex_escapes_everything`, `controls_are_visible`; ct: `ui_export_html_content`, `ui_export_tex_content`; `editing.rs`: `bidi_controls_are_found`, `invisible_characters_are_listed`; `browse.rs`: `display_name_escapes_controls`, `deceptive_names_keep_their_real_path`; `ferny-config/tests/i18n.rs`: `translations_keep_placeholders`, `translations_are_complete`; ct: `ui_smoke_bidi_warning` (bidi controls counted in a Trojan Source fixture); `ferny-build/tests/build.rs`: `output_is_neutralised` |
| T8 Single-instance IPC | `session.rs`: `ipc_request_round_trip`, `rejects_oversized_request`, `rejects_too_many_files`, `rejects_unknown_kind`, `rejects_relative_path`, `rejects_nul_and_control`, `socket_path_length_is_checked`, `socket_dir_is_created_private`, `socket_dir_symlink_is_refused`, `stale_socket_is_reported_as_ours`, `foreign_or_unexpected_socket_is_refused`; fuzz: `ipc_request`; ct: `ui_ipc_forward` |
| T9 Safe save | ct: `ui_split_shared`, `ui_split_close` (split view edits the shared document); `save.rs`: all 15 tests (atomic rename, no temp files left, mode kept, hard links kept, symlinks, read-only, external modification, failure keeps the original) |
| T10 Leaks via errors, session, logs | `backup.rs`: `copies_are_private_and_bounded`, `nothing_to_back_up_and_linked_store`; ct: `ui_save_actions_backup`, `ui_save_actions_off`; `document.rs`: `error_messages_carry_no_path`; `session.rs`: `errors_carry_no_path`, `session_file_is_owner_only`, `session_missing_and_invalid`, `session_drops_invalid_entries_and_clamps`, `scribble_is_bounded`; `browse.rs`: `errors_carry_no_path`; fuzz: `session`; logs: `ferny-core/tests/log.rs`: `lines_are_escaped_and_bounded`, `log_is_bounded_and_private` |
| T11 Geany import | `ferny-config/tests/geany.rs`: `does_not_import_commands`, `names_cannot_escape`, `limits_and_special_files`, `report_lists_unmapped`, `missing_directory_imports_nothing`; `ferny-syntax/tests/user.rs`: `user_catalog_rejects_invalid_files`; fuzz: `geany_import`; ct: `ui_geany_import` (through the menu command) |
| T13 `.editorconfig` and modelines | `editorconfig.rs`: `ignores_unknown_keys`, `rejects_bad_values`, `nearer_files_and_later_sections_win_and_root_stops`; `catalog.rs`: `detection_by_modeline_as_in_geany` |
| T14 Supply chain | `tools/ferny-sbom/tests/sbom.rs`: `sbom_lists_crates_vendored_code_and_qt`; Gate: `cargo deny`, `cargo audit`, `third_party integrity` (SHA-256 manifests); packaging: offline build from `git archive` (`scripts/package-macos.sh`) |
| T15 Fake or tampered binaries | sh: `package_macos_test.sh` (ref and tag checks, artefacts never overwritten); packaging verification: ad-hoc signature, arm64 only, no library or rpath outside the bundle, plugins only from the bundle, smoke test from the mounted DMG |
