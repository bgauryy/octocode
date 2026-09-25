# References

Load when a claim in this skill needs its source, or a pinned-version fact must be re-checked. Why: API details (cancellation, node-store defaults) changed across versions; the pinned source is the authority.

## Upstream crate sources inspected (pinned versions, in the local cargo registry source cache)

| Crate | Path | Notes |
|---|---|---|
| tree-sitter 0.27.0 | `binding_rust/lib.rs` | `ParseOptions::progress_callback` (no `set_timeout_micros`); `Parser` `Send+Sync`; `Node` Copy + `id()` semantics; `parent()` search note; cursor API; `QueryCursor` match limit, byte ranges, max start depth; built-in text predicates vs `general_predicates` |
| oxc_allocator 0.150.0 | `src/allocator.rs`, `src/pool/mod.rs` | `reset()` keeps largest chunk; no-`Drop` rule; `AllocatorPool::new(n).get()` |
| oxc_semantic 0.150.0 | `src/builder.rs`, `src/node/nodes.rs`, `src/scoping.rs` | `with_build_nodes` off by default; `new_compiler`/`new_linter`; `AstNodes` parent/ancestor API; `Scoping` ids |
| oxc_ast_visit / oxc_traverse 0.150.0 | `src/generated/visit.rs`, `src/lib.rs` | `visit_*` + `walk::walk_*`; `Traverse` ancestors; `Utf8ToUtf16` |
| oxc_parser / oxc_codegen 0.150.0 | `src/lib.rs`, `src/options.rs` | `ParserReturn` fields; codegen comment loss |
| ast-grep-core 0.45.3 | `src/node.rs`, `src/matcher/pattern.rs`, `src/match_tree/strictness.rs`, `src/tree_sitter/{mod,traversal}.rs` | `Root::edit` full reparse; strictness levels; `potential_kinds` prefilter; iterative `TsPre` traversal |

## Local sources

| File | Path | Notes |
|---|---|---|
| Engine AST code | `packages/octocode-native/crates/engine/src/{structural,signatures,text,search}` | house helpers and defect list in `octocode-engine-map.md` |
| Rewrite runtime | `packages/octocode-native/crates/runtime/src/tools/ast_rewrite/` | rewrite protocol |
| Engine docs | `packages/octocode-native/docs/engine/{SUPPORTED_LANGUAGES_AND_FEATURES,CODE_GRAPH,LSP_AST_AUDIT_FINDINGS,DEPENDENCY_AUDIT}.md` | contracts, durable ids, dependency rationale |
| Sibling skill ref | `rust-best-practices` → parsing-and-codegen | earlier short form of the parser/offset/rewrite rules |

## Background (not re-fetched)

| Topic | Source | Why it matters |
|---|---|---|
| Red/green trees | rust-analyzer `docs/book/src/contributing/syntax.md` (rowan) | alternative handle model: immutable green tree, on-demand red nodes with parents/offsets |

## Added in the 2026-09-24 improvement pass

| Source | Path or URL | Why it matters |
|---|---|---|
| tree-sitter C core 0.27.0 | `lib/src/{language,query,node,lexer}.c` in the crate | `id_for_node_kind` supertype and ERROR-prefix behavior, match-limit eviction, `child(i)` linear scan, BOM skipping |
| grammar crates | `tree-sitter-typescript-0.23.2/*/src/parser.c` (`LANGUAGE_VERSION 14`), javascript/python 0.25 (15) | mixed ABI; `name()`/`supertypes()` empty on ABI 14; tsx vs ts node-types |
| ast-grep-config 0.45.3 | `src/rule/{mod,selector,stop_by,nth_child,range,referent_rule}.rs`, `src/transform/mod.rs`, `src/fixer.rs` | rule order, selectors, contextual patterns, constraints, transform, fixer ranges |
| ast-grep-core 0.45.3 | `src/match_tree/{mod,strictness}.rs`, `src/meta_var.rs`, `src/source.rs` | structural metavariable equality, `$$A`, Smart strictness, O(line) column |
| oxc_parser / oxc_span / oxc_syntax / oxc_ecmascript / oxc_str 0.150.0 | `lib.rs` ParseOptions, `source_type.rs`, `module_record.rs`, `symbol.rs`, `ident.rs` | `module_record`, `SourceType::from_path`, `preserve_parens`, `Ident`/`Str`, spec helpers |
| oxc_ast / oxc_allocator / oxc_semantic 0.150.0 | `ast/comment.rs`, `trivia.rs`, `clone_in.rs`, `node/store.rs`, `lib.rs` features | `program.comments` + `attached_to`, `ContentEq`/`CloneIn`, NodeId panic without the store, `pool`/`serialize` features |
| oxc linter | github.com/oxc-project/oxc `crates/oxc_linter/src/{rule.rs,lib.rs}` | one flat pass bucketed by `AstType`; debug differential check |
| ruff | github.com/astral-sh/ruff `crates/ruff_python_ast/src/visitor/source_order.rs`, `crates/ruff_source_file/src/line_index.rs` | `TraversalSignal` pruning; shared LineIndex with encodings + BOM |
| rust-analyzer | `crates/syntax/src/ptr.rs`, `crates/span/src/ast_id.rs` | `SyntaxNodePtr` valid for same text only; `AstIdMap` edit-stable ids |
| Zed / Helix / Difftastic | `crates/language/src/syntax_map.rs`; `helix-editor/tree-house`; `src/parse/tree_sitter_parser.rs` | match limits 64/256, 16 KiB containing windows, included-range reset, layer handling, ERROR-threshold fallback |
| tree-sitter docs | tree-sitter.github.io using-parsers/queries, cli/fuzz | query operators, anchors, `tree-sitter fuzz` |

## Live dogfood evidence

| Fixture run | Result |
|---|---|
| `astSearch match foo($A)` over `scripts/adversarial-fixtures.mjs` output (ts, py) | emoji UTF-16 columns correct; BOM line-1 column 1 before the LineIndex fix, 0 after; huge-line and >1 MB → `ast.policy.inputTooLarge`; NUL → `ast.policy.binaryContent`; 20k-deep nesting and invalid UTF-8 matched without crash; 70k TS matches returned |
