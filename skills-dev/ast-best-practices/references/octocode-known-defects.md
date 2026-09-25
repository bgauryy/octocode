# octocode engine: known AST defects

Load before copying a pattern from engine AST code or when fixing one of these. Why: each fixed item is a guard that looks removable in review; removing it reopens the bug. Items are anchored by symbol; `grep -n` before citing a line.

## Open

None known. Watch items:
- oxc parse and `SemanticBuilder` can't be interrupted mid-call; cancellation takes effect after them (the input-size cap bounds a single parse).
- Pool timeouts include queue wait, so a saturated pool can time a job out before it starts.
- Not a bug: "oxc failure check ignores `fatal_error`". A fatal error leaves an empty program plus diagnostics, which the existing check already catches.

## Fixed (guards to keep)

| # | Guard | Regression test |
|---|---|---|
| 1 | BOM: `text/utf8_offsets.rs::LineIndex` hides a leading U+FEFF from row-0 UTF-16 columns (bytes unchanged); the LSP resolver and `search/classify.rs` share it, and ripgrep strips the BOM itself, so all surfaces agree | `line_index_hides_leading_bom_from_row_zero_columns`, `bom_bearing_files_map_ripgrep_columns_onto_the_matched_text` |
| 2 | `js_oxc.rs::source_type_for` uses `SourceType::from_path`, with JSX on for plain JS (React-in-`.js`, `.cts`, declaration variants) | `source_type_follows_the_full_path`, `jsx_in_plain_js_files_parses` |
| 3 | `search/classify.rs::position_to_byte` walks UTF-16 units (ripgrep's unit), not chars | `match_columns_are_utf16_like_ripgrep_reports_them` |
| 4 | astRewrite compiles the rule once per parser language (`CompiledRewrite`), parses with the deadline-bound parser, and checks the deadline between matches | `compiled_rewrite_is_shareable_and_reusable` |
| 5 | Rewrite `kind_to_id` and octo `structural/kinds.rs::named_kind_id` reject hidden supertypes, `ERROR` prefixes, and unknown or anonymous kinds, so they fail loudly | `rule_kind_rejects_supertypes_and_error_prefixes`, `rejects_supertypes_error_prefixes_and_unknown_kinds` |
| 6 | `rewrite_parser_for_path` is shared by the scan and the syntax-regression check, so `.tsx` is error-counted with the TSX grammar | — |
| 7 | Cursor iteration, not `child(i)` loops (`octo/matching.rs`, `syntax_tree.rs`, `count_syntax_errors`) | — |
| 8 | Syntax-tree paging: recovered (`partial`) files paginate; only the requested window is materialized | `recovered_files_still_paginate` |
| 9 | oxc entry points reject non-JS/TS paths before copying content or spawning a thread | `non_js_files_are_rejected_before_any_oxc_work` |
| 10 | Directory `astSearch match` runs files in parallel with one matcher per extension and keeps output order | — |
| 11 | oxc runs on one shared 64 MB-stack rayon pool (`deep_stack.rs`, `available_parallelism` clamped 2..16) with a per-thread `Allocator` (reset per job, replaced above 16 MB) and a cooperative cancel flag checked in the walk | `returns_the_job_result_on_a_large_stack_pool_thread`, `nested_deep_stack_calls_run_inline_instead_of_deadlocking`, `thread_allocator_is_reused_and_reset_between_jobs`, `call_walk_stops_when_the_job_is_cancelled` |
| 12 | The call walker is an `oxc_ast_visit::VisitJs` visitor (`js_oxc_calls.rs`): no `_ => {}` drops | `call_order_and_owners_match_the_legacy_walker` |
| 13 | astRewrite parses each matched file once for preview and once for apply (plus one with `selectedMatchIds`); one parse feeds both the regression check and the postcondition | — |
| 14 | The LSP anchor resolver keeps an ancestor stack (no `parent()` walks); a candidate is a declaration only when it fills a naming field (`name`, `declarator`, `key`, `left`, `pattern`, `label`) of a declaring node | `class_method_field_and_variable_names_are_declarations` |
| 15 | One tree-sitter parse helper, `extractor.rs::parse_with_deadline` (thread-local parser, reset before and after); `parse_before` and `parse_tree_with_deadline` are thin wrappers | — |
| 16 | Matcher: `CaptureEnv` checkpoint/rollback undo log, borrowed `MetaVar<'a>`, `MatchWithKind` stores kind ids | `capture_env_rollback_restores_inserts_and_replacements` |
| 17 | `<$T>` matches JSX in jsx/tsx/js grammars only (dead HTML-tag and `block_mapping_pair` paths removed) | `jsx_tag_pattern_matches_elements_only_in_jsx_grammars` |
| 18 | `LineIndex` column math uses UTF-16 checkpoints every 1 KiB plus an all-ASCII fast path, so minified single-line files stay linear | `checkpointed_columns_match_a_naive_scan_everywhere`, `a_minified_single_line_stays_linear` |
| 19 | astTopology `deadCode` value escapes come from syntax, not word counts: oxc resolved references per declaration symbol (`js_oxc_references.rs`) and identifier tokens by name for tree-sitter (`graph_facts::count_name_references`); declaration names, export clauses, and call targets are excluded, and counts are keyed by declaration id | `comments_and_strings_are_not_references`, `a_shadowing_local_does_not_reference_the_export`, `rust_comments_and_strings_are_not_references`, `names_only_in_comments_or_strings_do_not_keep_exports_live` |

Fix a new issue with the rules in `references/efficiency.md` and `references/walking.md`, then verify with the steps in `references/octocode-engine-map.md`.
