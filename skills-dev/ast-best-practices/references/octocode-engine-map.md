# octocode engine AST map

Load when editing AST code under `packages/octocode-native/crates/engine/src` or `runtime/src/tools/ast_rewrite`. Why: the house helpers already solve reuse, deadlines and offsets. Copying a known-bad pattern spreads a defect. Anchored by symbol; `grep -n` before citing a line.

## Dependency baseline

The engine uses `oxc_allocator`, `oxc_ast`, `oxc_parser`, `oxc_semantic` and `oxc_span` at `=0.150.0` with no extra features, plus `tree-sitter 0.27`, the grammars, and (behind a feature flag) `ast-grep-core/config 0.45.3`. `Visit`, `Utf8ToUtf16`, `AllocatorPool`, `Traverse` and linter node bitsets each need a new crate or feature first (`references/oxc-toolkit.md`). Adding one needs consent.

## Which stack does what

| Surface | Stack | Entry |
|---|---|---|
| astSearch structural (pattern, rule) | tree-sitter + in-house **octo** matcher (not ast-grep) | `structural/octo/{mod,pattern,rule,matching}.rs`, `structural/files.rs` |
| astSearch syntax tree | tree-sitter, explicit stack, preorder `u32` ids + `parent_id` | `structural/syntax_tree.rs` |
| astRewrite | **ast-grep** (feature `embedded-ast-grep-rewrite`) + runtime guard protocol | `structural/rewrite.rs`, `runtime/src/tools/ast_rewrite/` |
| Signatures / minify skeleton | tree-sitter `@body` queries | `signatures/{extractor,languages}.rs` |
| Graph facts | tree-sitter Enter/Exit frames + cursor | `signatures/graph_facts/mod.rs` |
| JS/TS symbols, refs, CommonJS | **oxc** parser + semantic | `signatures/js_oxc*.rs` |
| Language registry | one `LazyLock` table | `signatures/languages.rs` (`LANGUAGE_TABLE`, `find_entry`) |

## Reuse these; don't reimplement them

| Need | Helper |
|---|---|
| Parse with deadline and a parser per thread | `signatures/extractor.rs::parse_with_deadline` (`parse_before` and `octo/matching.rs::parse_tree_with_deadline` wrap it) |
| Cached compiled `Query` | `extractor.rs::cached_query` (rejects general and property predicates) |
| Bounded query run | `extractor.rs`: match limit 65 536 + progress deadline + `did_exceed_match_limit` → `None` |
| Iterative named preorder | `structural/octo/matching.rs::visit_named` |
| Byte → line/col, UTF-16 | `text/utf8_offsets.rs::LineIndex` (via `octo/line_index_support.rs`, `js_oxc_shared.rs`) |
| Big stack + panic containment for oxc | `signatures/deep_stack.rs` |
| Timeout constant | `AST_EXECUTION_TIMEOUT` (2s) |
| Rewrite safety | `ast_rewrite/mod.rs`: overlap reject, expected-text splice, syntax-regression reparse, hash gates, journal |

## Position contract per surface

| Surface | Line base | Column |
|---|---|---|
| astSearch octo structural, syntax_tree | 1 | 0-based UTF-16 |
| graph_facts `range`, oxc outputs | 0 (graph_facts also emits a 1-based `line`) | UTF-16 |
| astRewrite public `range` | 1 | 0-based UTF-16 (`structural/rewrite.rs` converts from ast-grep's scalar count) |
| signatures | 1 | none |

A new surface must pick one row on purpose and document it. Mixing rows is the most common source of off-by-emoji bugs.

Before copying any pattern from this code, load `references/octocode-known-defects.md`. It lists the audited defects that must not spread.

## Verify a change

1. Run `yarn workspace @octocodeai/octocode-native test:rust`.
2. Run `yarn workspace @octocodeai/octocode-native build:dev`.
3. Generate fixtures **inside the workspace**, because tools refuse paths outside allowed roots (`ast.policy.outsideAllowedRoots`): `node skills-dev/ast-best-practices/scripts/adversarial-fixtures.mjs --out "$PWD/.octocode/tmp/ast-best-practices/fx" --lang ts,py`.
4. Run the real tool on each fixture:
   ```bash
   node packages/octocode/out/octocode.js astSearch '{"queries":[{"reasoning":"fixture","operation":"match","path":"<abs fixture>","pattern":"foo($A)","langType":"typescript","captureText":true}]}'
   ```
5. Expected outcomes:
   - emoji: UTF-16 columns.
   - bom: line-1 column 0.
   - huge-line: `ast.policy.inputTooLarge`.
   - nul-byte: `ast.policy.binaryContent`.
   - deep-nesting and invalid-utf8: a match and no crash.
   - many-matches (TS): 70 000 matches.

This step ends the change. General rules are in `references/efficiency.md`; the fixture rationale is in `references/testing.md`.
