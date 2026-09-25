---
name: ast-best-practices
description: "Use when writing, reviewing, or debugging Rust code that parses source into syntax trees and works on them with tree-sitter, oxc, or ast-grep. Covers grammar and source-type choice, node handles and identity, walking and visitors, queries, patterns and rules, positions and offsets (UTF-16, BOM, CRLF), byte-range rewrites, parse cost and limits, and adversarial testing. Typical requests: add an AST walker, a tree-sitter query, an oxc visitor, parent or ancestor lookup, node ids, a structural pattern or ast-grep rule that matches nothing, a rewrite that breaks syntax, wrong line or column, a parse that is slow, times out or blows the stack, JSX or TS that fails to parse. Not for calling astSearch or astRewrite as a user (octocode-research) or general Rust idioms (rust-best-practices)."
---

# AST Best Practices

tools: `npx octocode` / `octocode-mcp`
related-skill: `rust-best-practices`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

This skill is for writing tree code that is correct on malformed input, bounded on hostile input, and cheap across thousands of files. It targets the stacks in `octocode-native/crates/engine`: **tree-sitter 0.27**, **oxc 0.150** and **ast-grep-core 0.45**. Each claim has been checked against that pinned source.

Flow: `FRAME → INSPECT → APPLY → VERIFY`.
- **FRAME**: name the stack and the operation (grammar, handle, walk, query, rewrite, perf, test).
- **INSPECT**: before writing new tree code, read the repo's existing helper and the pinned crate source (`~/.cargo/registry/src/*/<crate>-<ver>/`). Never trust API memory across versions.
- **APPLY**: make the smallest change that reuses the house helper. Adding a crate or feature needs consent.
- **VERIFY**: run tests on the adversarial fixtures, then `cargo clippy`. For engine changes, also run the real tool.

Reports go to `<output>/ast-best-practices/`. Scratch work and fixtures go to `<output>/tmp/ast-best-practices/`. Advice that fits in chat stays in chat.

**Stop when** the change reuses a house helper, every lobby rule it touches holds, and the fixture run behaves as expected. **Escalate** when the fix needs a new crate or feature, changes a public position contract, or conflicts with a known defect: report it rather than working around it.

## Mental model

- **Parse, then borrow.** tree-sitter `Node<'tree>` and oxc `&'a T` are views into a tree or arena. They can't outlive it, can't survive an edit, and aren't durable ids.
- **Bytes inside, users outside.** Every stack stores **byte** offsets. Convert to line/column (which line base? UTF-16? BOM?) exactly once, at the output boundary, through one `LineIndex`.
- **Every tree may be partial.** tree-sitter always returns a tree with `ERROR`/`MISSING` nodes, and oxc recovers with diagnostics. Decide explicitly whether a broken tree is acceptable.
- **Bound everything.** Input size, parse time, walk depth, match count and output size. Untrusted input will hit each one.

## Pick the stack

| Need | Use | Why |
|---|---|---|
| JS/TS imports and exports | oxc `ParserReturn.module_record` | computed by the parser, no walk |
| JS/TS symbols, scopes, references, globals | **oxc** + `oxc_semantic` `Scoping` | real binding resolution; tree-sitter only has names |
| Multi-language syntax, signatures, structural search | **tree-sitter** (house `octo` matcher) | one API across 11 language families |
| Fixed-shape capture per language (bodies, highlights) | tree-sitter `Query` | compile once; the binding evaluates text predicates |
| Rewrite with metavariable templates | **ast-grep** (`embedded-ast-grep-rewrite`) | fixer and template engine; guard it yourself |
| Symbol identity, cross-file references | **not an AST**: use LSP | AST edges are candidates, not proof |

## Lobby rules: the do / don't core

1. **Reuse the parser machinery.**
   - tree-sitter: one `Parser` per thread (`thread_local! RefCell<Parser>`), with `reset()` before each file, because a cancelled parse otherwise **resumes**. Also clear included ranges if they were ever set.
   - oxc: one `Allocator` per thread, `reset()` per file.
   - One compiled `Query`, pattern or `RuleConfig` per request, shared through `Arc`.
   - Never construct any of these per file or per match.
2. **Put a deadline and a size cap on every parse and query.** In tree-sitter 0.27 the only cancel is `progress_callback` returning `ControlFlow::Break`; there is no `set_timeout_micros`. When `did_exceed_match_limit()` fires, treat the result as incomplete: matches were dropped mid-run.
3. **Choose the grammar and source type on purpose.** `tsx` ≠ `typescript`. Use oxc `SourceType::from_path`, not a hand-made map. Validate every kind name with `id_for_node_kind`: `0`, supertype names (`expression`) and `ERROR` prefixes silently match nothing or the wrong nodes.
4. **Walk iteratively.** Use one `TreeCursor`, an explicit stack, or oxc's flat `AstNodes` loop. Recursion over user-controlled nesting needs a depth cap or a big-stack thread. Never loop over `node.child(i)` (O(k²)) or call `node.parent()` repeatedly (it searches down from the root each time).
5. **Keep node identity out of long-lived state.** tree-sitter `id()`, oxc `NodeId` and `SymbolId` are scoped to one tree. `(path, byte range, kind)` is valid only with the same content hash, so store the hash too.
6. **Take text as a slice** (`src.get(node.byte_range())`), not `utf8_text` plus `to_string()`. In hot loops, compare `kind_id()` or cached ids, not strings.
7. **Handle errors explicitly.** Check tree-sitter `has_error()`/`is_missing()` and oxc `fatal_error` plus `diagnostics`. Never bind a metavariable to a `MISSING` node. ast-grep `Pattern::has_error()` checks only the root.
8. **Pay only for what you read.** `SemanticBuilder::new()` builds no `AstNodes`, and `parent_id`/`kind(id)` then **panic**. Add `with_build_nodes(true)` only when you need them, and don't default to `new_linter()`. Get facts from built-ins (`module_record`, `program.comments`, `Scoping`) before walking.
9. **Rewrite by span, validate by reparse.**
   - Keep edits non-overlapping and apply them back to front.
   - Check the expected old text is still at each range.
   - Reparse the result and reject any new error nodes.
   - For minimal diffs, splice spans; don't use oxc `Codegen`, which loses formatting and expression-level comments.
10. **Prefilter before parsing, anchor before matching.** Filter on extension, then a ripgrep literal, then kind. Every ast-grep or octo rule branch needs a `kind` or pattern anchor, because regex, text and relational-only branches try every node.

## Smart routes: load only what the current step needs

- When choosing or validating a grammar, kind name, `SourceType` or `ParseOptions`, adding a language, or parsing injected or embedded code, load `references/grammar-and-language.md`. It covers ABI, tsx vs ts, supertype and ERROR kind traps, aliases, 0.27 API changes and included ranges.
- When a question is about what a node *is* or how to hold it (CST vs AST, fields, errors, ids, durability, position units, BOM), load `references/concepts-and-handles.md`.
- When writing a tree-sitter traversal (cursor loop, pruning, enter/exit, ancestors), load `references/walking.md`. For oxc `Visit`/`VisitMut`/`Traverse`, flat `AstNodes` loops, or recursion over hostile nesting, load `references/walking-oxc-and-depth.md`.
- When writing JS/TS analysis on oxc (imports, comments or JSDoc, globals, symbol flags, comparing or cloning or building nodes, many checks per file, needed Cargo features), load `references/oxc-toolkit.md`. oxc usually already computes what you are about to hand-roll.
- When writing a tree-sitter `Query`, load `references/tree-sitter-queries.md`. It covers compile-once, the match-limit eviction model, the full predicate list, query operators and compile errors.
- When writing an ast-grep pattern or rule, or an octo structural pattern, load `references/patterns-and-rules.md`. It covers strictness, metavariable semantics, rule order, `stopBy`, selectors, contextual patterns, constraints, kind anchors and the octo expando and fragment repair.
- When mutating code (astRewrite, fixers, codemods, incremental reparse), load `references/editing.md` for the rewrite protocol.
- When a parse or walk is slow, allocation-heavy, runs out of memory or overflows the stack, load `references/efficiency.md`. It has the reuse table, bounds, one-pass design and caching keys, and is measure-first.
- When adding or changing tree code, before claiming it works, load `references/testing.md`. Then run `scripts/adversarial-fixtures.mjs --out <dir> [--lang ts,py]` (see `--help`), which writes emoji, CRLF, BOM, broken, deep-nesting, huge-line, invalid-UTF-8, NUL and many-match fixtures plus a manifest.
- When touching `octocode-native` engine AST code, load `references/octocode-engine-map.md`. It covers the dependency baseline, which helper to reuse, the position contract per surface and the fixture-run recipe. Before copying an existing engine pattern, load `references/octocode-known-defects.md`.
- When reviewing, use the numbered lobby rules as the checklist. Report `rule # → file:line → failure on which fixture → fix`, ordered by severity (panic or hang, then wrong output, then cost).
- For crate behavior beyond these notes, read the pinned registry source. Use `octocode-research` for GitHub history. `references/references.md` lists the sources behind each claim.

## Related routes

- Use `rust-best-practices` for general idioms, FFI panic boundaries and subprocess work. Use `octocode-research` to trace callers or upstream code. Use `octocode-eval-benchmark` to prove a performance change. Use `octocode-clean-agentic-code` to remove dead walkers or patterns.
