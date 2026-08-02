# Graph Report - .  (2026-08-02)

## Corpus Check
- 158 files · ~215,468 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1389 nodes · 2977 edges · 81 communities (63 shown, 18 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 123 edges (avg confidence: 0.79)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Frank App Components
- Frank State Components
- Frank Target Components
- Frank Pack Components
- Frank Pack Components
- Frank Compress Components
- Frank Safeio Components
- Frank Target Components
- Frank Gui Components
- Package.Json Components
- Frank Target Components
- Frank Mcp Components
- Frank Compress Components
- Xtask Components
- Frank Gui Components
- Frank Compress Components
- Frank Gui Components
- Xtask Components
- Frank Ledger Components
- Frank Gui Components
- Frank Ledger Components
- Frank Safeio Components
- Frank Target Components
- Frank Target Components
- Frank Safeio Components
- Frank Cli Components
- Frank Ledger Components
- Frank Ledger Components
- Frank Gui Components
- Frank Cli Components
- Frank Cli Components
- Frank Ledger Components
- Frank Ledger Components
- Frank Gui Components
- Frank Cli Components
- Frank Cli Components
- Frank Ledger Components
- Frank Gui Components
- Frank Compress Components
- Frank Gui Components
- Frank App Components
- Frank Cli Components
-  Components
- Packs Components
- Frank Ledger Components
- Frank Gui Components
- Frank Compress Components
- Frank Pack Components
- Frank Gui Components
- Frank Cli Components
- Bench Components
- Frank Compress Components
- Scripts Components
- Frank Gui Components
- Frank Gui Components
- Frank Gui Components
- Frank Gui Components
- Frank Gui Components
- Frank Gui Components
- Scripts Components
- Scripts Components
- Scripts Components
- Scripts Components
- Scripts Components
- Scripts Components
- Scripts Components
- Scripts Components
- Scripts Components

## God Nodes (most connected - your core abstractions)
1. `CompiledPack` - 46 edges
2. `FrankService` - 39 edges
3. `AppError` - 26 edges
4. `InstallPlan` - 24 edges
5. `validate()` - 23 edges
6. `paths()` - 22 edges
7. `compile()` - 22 edges
8. `service()` - 19 edges
9. `fixture_pack()` - 19 edges
10. `attribute_by_mode()` - 18 edges

## Surprising Connections (you probably didn't know these)
- `AppState` --references--> `FrankService`  [EXTRACTED]
  apps/frank-gui/src-tauri/src/lib.rs → crates/frank-app/src/lib.rs
- `service()` --references--> `FrankService`  [EXTRACTED]
  apps/frank-gui/src-tauri/src/lib.rs → crates/frank-app/src/lib.rs
- `apply_prepared_plan()` --references--> `OperationResult`  [EXTRACTED]
  apps/frank-gui/src-tauri/src/lib.rs → crates/frank-app/src/lib.rs
- `apply_prepared_pack()` --references--> `PackOperationResult`  [EXTRACTED]
  apps/frank-gui/src-tauri/src/lib.rs → crates/frank-app/src/lib.rs
- `emit_rust()` --references--> `CompiledPack`  [EXTRACTED]
  xtask/src/main.rs → crates/frank-pack/src/compiler.rs

## Import Cycles
- None detected.

## Communities (81 total, 18 thin omitted)

### Community 0 - "Frank App Components"
Cohesion: 0.07
Nodes (77): Arc, AtomicU64, builtin_pack(), action_paths(), active_level_is_canonical_and_off_is_explicit(), AppError, changing_settings_makes_a_prepared_target_plan_stale(), Clock (+69 more)

### Community 1 - "Frank State Components"
Cohesion: 0.06
Nodes (78): add(), build(), builtin(), current(), current_or_builtin(), is_remote_source(), level_by_id(), list() (+70 more)

### Community 2 - "Frank Target Components"
Cohesion: 0.06
Nodes (54): ClaudeCodeTarget, quote(), Path, PathBuf, String, Vec, command_version_output(), command_version_probe_is_checked_with_stdout_and_stderr() (+46 more)

### Community 3 - "Frank Pack Components"
Cohesion: 0.10
Nodes (52): collapse_blank_runs(), compile(), compile_activation(), CompiledActivation, CompiledLevel, CompiledOneshot, normalize(), PackSource (+44 more)

### Community 4 - "Frank Pack Components"
Cohesion: 0.12
Nodes (40): PackError, Box, Error, PathBuf, String, add_compile_select_and_remove_local_pack(), changed_locked_copy_is_refused(), collect_files() (+32 more)

### Community 5 - "Frank Compress Components"
Cohesion: 0.06
Nodes (24): backup_dir_for(), backup_path_for(), data_home(), Path, PathBuf, code_patterns(), detect_file_type(), FileClass (+16 more)

### Community 6 - "Frank Safeio Components"
Cohesion: 0.08
Nodes (32): all_valid_modes_round_trip_through_symlinked_parent(), append_line(), append_line_writes_and_appends_with_single_trailing_newline(), concurrent_appends_yield_well_formed_lines(), creates_parent_directory_when_missing(), ensure_dir(), flag_file_permissions_are_0600_through_symlink(), home_dir() (+24 more)

### Community 7 - "Frank Target Components"
Cohesion: 0.09
Nodes (41): build_install_plan(), build_uninstall_plan(), ctx(), markdown_block_resolves_project_relative_path(), plan_scope_rejects_parent_traversal_before_apply(), Fn, Option, String (+33 more)

### Community 8 - "Frank Gui Components"
Cohesion: 0.05
Nodes (44): app, security, windows, build, beforeBuildCommand, beforeDevCommand, devUrl, frontendDist (+36 more)

### Community 9 - "Package.Json Components"
Cohesion: 0.04
Nodes (44): @axe-core/playwright, devDependencies, @axe-core/playwright, fast-check, jsdom, @playwright/test, @testing-library/jest-dom, @testing-library/react (+36 more)

### Community 10 - "Frank Target Components"
Cohesion: 0.08
Nodes (40): hook(), add_command_hook(), has_marker(), hooks_array(), HookSpec, prune_orphaned(), read_settings(), remove_owned_hooks() (+32 more)

### Community 11 - "Frank Mcp Components"
Cohesion: 0.10
Nodes (37): matches_legacy_js_compressor_on_real_compress_fixtures(), matches_legacy_js_compressor_on_representative_cases(), repo_root(), Option, PathBuf, String, run_js_oracle(), client_forwarding_is_byte_for_byte() (+29 more)

### Community 12 - "Frank Compress Components"
Cohesion: 0.10
Nodes (36): bullet_regex(), count_bullets(), counter(), extract_code_blocks(), extract_headings(), extract_inline_codes(), extract_paths(), extract_urls() (+28 more)

### Community 13 - "Xtask Components"
Cohesion: 0.13
Nodes (35): ExitStatus, archive_name(), binary_name(), build_one_pack(), build_packs(), build_packs_compiles_only_pack_directories(), check_path_scope(), checksums() (+27 more)

### Community 14 - "Frank Gui Components"
Cohesion: 0.09
Nodes (26): api, App(), isDashboardSnapshot(), labels, Page, Settings(), baseSnapshot, inactiveSnapshot (+18 more)

### Community 15 - "Frank Compress Components"
Cohesion: 0.12
Nodes (32): articles(), collapse_blank_runs(), collapse_spaces(), compress(), compress_prose(), CompressResult, fillers(), hedges() (+24 more)

### Community 16 - "Frank Gui Components"
Cohesion: 0.20
Nodes (31): AppHandle, add_local_pack(), apply_prepared_pack(), apply_prepared_plan(), AppState, doctor(), prepare_pack_change(), prepare_target_change() (+23 more)

### Community 17 - "Xtask Components"
Cohesion: 0.15
Nodes (21): Map, check(), check_values(), Limits, Metric, metric_summary(), Metrics, nonnegative_integer() (+13 more)

### Community 18 - "Frank Ledger Components"
Cohesion: 0.12
Nodes (22): find_recent_session(), parse_iso8601_ms(), parse_session(), RawEntry, RawMessage, RawUsage, Option, Path (+14 more)

### Community 19 - "Frank Gui Components"
Cohesion: 0.08
Nodes (25): compilerOptions, allowJs, allowSyntheticDefaultImports, esModuleInterop, forceConsistentCasingInFileNames, isolatedModules, jsx, lib (+17 more)

### Community 20 - "Frank Ledger Components"
Cohesion: 0.15
Nodes (25): aggregate_history(), append_history(), estimate_component(), HistoryRow, lifetime_verdict_has_enough_data(), measured_input_total(), measured_output_total(), read_history() (+17 more)

### Community 21 - "Frank Safeio Components"
Cohesion: 0.34
Nodes (18): append_line(), ensure_dir(), open_append_create(), open_existing_verified_dir(), open_verified_dir(), open_verified_dir_inner(), read_flag_raw(), read_lines() (+10 more)

### Community 22 - "Frank Target Components"
Cohesion: 0.21
Nodes (16): append(), append_preserves_existing_user_content(), append_to_empty_file(), AppendOutcome, Block, each_begin_pairs_with_nearest_end_before_next_begin(), multiple_well_formed_blocks_all_removed(), orphan_begin_removes_only_the_marker_not_trailing_content() (+8 more)

### Community 23 - "Frank Target Components"
Cohesion: 0.20
Nodes (15): backup_marker_survives_when_no_settings_json_existed_before_first_install(), ctx(), doctor_reports_missing_then_present_hooks(), dry_run_plan_description_matches_what_apply_actually_does(), fresh_install_writes_both_hooks(), install_backs_up_settings_exactly_once(), install_is_idempotent_no_duplicate_entries(), install_refuses_a_settings_symlink_without_touching_the_target() (+7 more)

### Community 24 - "Frank Safeio Components"
Cohesion: 0.28
Nodes (15): append_line(), ensure_dir(), is_symlink_at(), read_flag_raw(), read_lines(), remove_file(), remove_file_if_contains(), Path (+7 more)

### Community 25 - "Frank Cli Components"
Cohesion: 0.27
Nodes (16): assert_success(), backup_path(), built_in_pack_path(), compression_check_dry_run_write_and_restore_are_safe(), frank(), install_and_uninstall_dry_run_cover_native_and_unknown_targets(), mcp_cli_reports_usage_and_spawn_failures_without_panicking(), pack_commands_cover_builtin_paths_and_fail_closed_errors() (+8 more)

### Community 26 - "Frank Ledger Components"
Cohesion: 0.22
Nodes (15): attribute_by_mode(), buckets_accumulate_all_four_token_fields_not_just_output(), direct_attribution_sorts_out_of_order_transition_rows(), flag_mtime_basis_excludes_tokens_before_the_write(), flag_mtime_before_first_turn_falls_back_to_whole_session(), huge_counters_saturate_instead_of_panicking_or_wrapping(), log_basis_attributes_each_span_to_the_mode_active_at_that_time(), log_basis_prefix_mode_can_be_a_real_mode_not_just_off() (+7 more)

### Community 27 - "Frank Ledger Components"
Cohesion: 0.21
Nodes (13): dispatch(), session_start(), statusline(), user_prompt_submit(), append(), injection_ledger_round_trips_and_ignores_unknown_kinds_for_totals(), InjectionEntry, read_all() (+5 more)

### Community 28 - "Frank Gui Components"
Cohesion: 0.13
Nodes (15): devDependencies, fast-check, @testing-library/jest-dom, @types/react, @types/react-dom, typescript, vite, @vitest/coverage-v8 (+7 more)

### Community 29 - "Frank Cli Components"
Cohesion: 0.23
Nodes (13): Cli, Command, guarded_hook(), main(), McpCommand, PackCommand, Command, Option (+5 more)

### Community 30 - "Frank Cli Components"
Cohesion: 0.32
Nodes (10): build_and_record(), explain_text(), lifetime_text(), report_text(), Option, Path, PathBuf, String (+2 more)

### Community 31 - "Frank Ledger Components"
Cohesion: 0.24
Nodes (10): Attribution, AttributionBasis, Event, BTreeMap, Option, String, TokenBucket, JsonBucket (+2 more)

### Community 32 - "Frank Ledger Components"
Cohesion: 0.26
Nodes (7): build_session_report(), minimal_pack(), render_text_handles_zero_turns_gracefully(), render_text_labels_measured_vs_estimated_separately(), render_text_reports_unmeasured_level_without_guessing(), render_text_top_line_total_includes_unattributed_tokens(), Path

### Community 33 - "Frank Gui Components"
Cohesion: 0.18
Nodes (11): dependencies, react, react-dom, @tauri-apps/api, @tauri-apps/plugin-autostart, @tauri-apps/plugin-dialog, react, react-dom (+3 more)

### Community 34 - "Frank Cli Components"
Cohesion: 0.25
Nodes (6): install(), on(), Option, TargetOperation, run_service_plan(), uninstall()

### Community 35 - "Frank Cli Components"
Cohesion: 0.27
Nodes (7): compress_one(), CompressArgs, restore_all(), Path, PathBuf, Vec, run()

### Community 36 - "Frank Ledger Components"
Cohesion: 0.31
Nodes (9): mode_log_filters_malformed_values_and_sorts_clock_skew(), ModeLogRow, normalize(), read_mode_log(), Option, Path, String, Value (+1 more)

### Community 37 - "Frank Gui Components"
Cohesion: 0.20
Nodes (9): description, identifier, permissions, $schema, windows, autostart:default, core:default, dialog:allow-open (+1 more)

### Community 38 - "Frank Compress Components"
Cohesion: 0.27
Nodes (9): compress(), compressDescriptionsInPlace(), compressProse(), FILLERS, HEDGES, LEADERS, PLEASANTRIES, PROTECTED_PATTERNS (+1 more)

### Community 39 - "Frank Gui Components"
Cohesion: 0.25
Nodes (8): scripts, build, dev, lint, tauri, test, test:ui, typecheck

### Community 40 - "Frank App Components"
Cohesion: 0.52
Nodes (6): app_rejects_non_directories_reserved_packs_and_bad_digests(), app_rejects_unknown_target_without_creating_state(), paths(), Path, PathBuf, write_pack()

### Community 41 - "Frank Cli Components"
Cohesion: 0.48
Nodes (6): every_hook_entrypoint_returns_zero_for_broken_environment_input(), frank(), hook_dispatch_happens_before_clap_construction(), malformed_hook_stdin_is_a_successful_noop(), Command, version_is_available_from_the_binary_adapter()

### Community 42 - " Components"
Cohesion: 0.33
Nodes (5): Error, From, Self, SafeIoError, Errno

### Community 43 - "Packs Components"
Cohesion: 0.38
Nodes (6): Activation, AliasEntry, Level, Oneshot, Reduction, Option

### Community 44 - "Frank Ledger Components"
Cohesion: 0.33
Nodes (4): format_usd(), price_for_model(), Option, String

### Community 45 - "Frank Gui Components"
Cohesion: 0.40
Nodes (4): name, private, type, version

### Community 46 - "Frank Compress Components"
Cohesion: 0.70
Nodes (4): is_sensitive_path(), Path, Regex, sensitive_basename_regex()

### Community 47 - "Frank Pack Components"
Cohesion: 0.60
Nodes (3): invalid_digest_and_duplicate_selector_are_rejected(), Path, write_pack()

### Community 48 - "Frank Gui Components"
Cohesion: 0.83
Nodes (3): ensure_dev_resource(), main(), Path

### Community 49 - "Frank Cli Components"
Cohesion: 1.00
Nodes (3): config_dir(), path(), PathBuf

## Knowledge Gaps
- **139 isolated node(s):** `name`, `private`, `version`, `type`, `dev` (+134 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **18 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `CompiledPack` connect `Frank State Components` to `Frank App Components`, `Frank Ledger Components`, `Frank Pack Components`, `Frank Pack Components`, `Xtask Components`, `Frank Ledger Components`?**
  _High betweenness centrality (0.152) - this node is a cross-community bridge._
- **Why does `InstallPlan` connect `Frank Target Components` to `Frank App Components`, `Frank Target Components`?**
  _High betweenness centrality (0.044) - this node is a cross-community bridge._
- **Why does `FrankService` connect `Frank App Components` to `Frank Gui Components`, `Frank State Components`?**
  _High betweenness centrality (0.036) - this node is a cross-community bridge._
- **What connects `name`, `private`, `version` to the rest of the system?**
  _139 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Frank App Components` be split into smaller, more focused modules?**
  _Cohesion score 0.06758199847444699 - nodes in this community are weakly interconnected._
- **Should `Frank State Components` be split into smaller, more focused modules?**
  _Cohesion score 0.059644322845417236 - nodes in this community are weakly interconnected._
- **Should `Frank Target Components` be split into smaller, more focused modules?**
  _Cohesion score 0.06044303797468355 - nodes in this community are weakly interconnected._