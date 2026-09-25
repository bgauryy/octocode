# Efficiency and bounds

Load when parse or walk cost matters, or when input is untrusted. Why: tools here parse thousands of files per call on a rayon pool, so a per-file allocation or a missing bound multiplies. Measure with `--release` or the `profiling` profile, and prove the gain with `octocode-eval-benchmark`, before you claim a win.

## Reuse table

| Object | Build | Reuse via | Anti-pattern |
|---|---|---|---|
| ts `Parser` | `Parser::new()` | `thread_local! RefCell<Parser>`. Per file: `reset()` (after a cancel, the next `parse` **resumes the old document**), `set_language()` (cheap), and `set_included_ranges(&[])` if ranges were ever set | `Parser::new()` per file; skipping `reset` |
| ts `Language` | grammar fn | a `LazyLock` table | reloading per file |
| ts `Query` | `Query::new` (costly) | `Arc<Query>` cache keyed by language + source | compiling per file |
| ts `TreeCursor` | `node.walk()` | `cursor.reset(node)`; pass `&mut cursor` to `children` | a fresh cursor per child list |
| oxc `Allocator` | `Allocator::default()` | one per thread with `reset()` per file, or `AllocatorPool::new(threads).get()` | a new allocator per file (repeated system allocations), or never resetting (grows without bound) |
| oxc `SemanticBuilder` | per program | `new()`/`new_compiler()`; opt in to `with_build_nodes`, `with_cfg`, `with_check_syntax_error` only when needed | `new_linter()` everywhere |
| ast-grep `Pattern` / `RuleConfig` | parse + compile | build once per request, share across `par_iter` | recompiling inside the per-file closure |

## Bounds that are always on

- **Size cap** before parsing. The house cap is 1 MB (`MAX_STRUCTURAL_CONTENT_BYTES`, `MAX_REWRITE_CONTENT_BYTES`). Skip minified and generated files.
- **Deadline** for parse, query and walk. The house value is `AST_EXECUTION_TIMEOUT = 2s`. Tree-sitter uses a progress callback, cursor walks check per node, and oxc runs on a big-stack thread with `recv_timeout`.
  - A timed-out detached thread **keeps burning CPU**. Prefer cooperative cancellation (an `AtomicBool` checked inside the visitor) where the stack allows it.
- **Match and output caps**, each reported as a limit, never as a clean end: `set_match_limit`, the house cap of 100k rewrite matches, and the syntax-tree cap of 1M nodes with paged output.
- **Depth caps** for any recursion (see walking).

## Allocation cuts

- Take text as a borrowed `&src[range]`. A single `utf8_text` call doesn't allocate, but it re-validates UTF-8 on every call. ast-grep `text()` returns a `Cow`, so don't call `.to_string()` unless you store the text.
- Compare kinds by `kind_id()`, using an `id_for_node_kind` lookup resolved once, not by string, and not by scanning every node kind in the language.
- Keep capture environments small and copy-cheap. Cloning two `HashMap`s per backtracking branch dominates the matcher profile; prefer an undo log or `SmallVec` of bindings.
- Don't `to_owned()` the source per entry point. Pass `&str` or an `Arc<str>` into the worker.
- Avoid O(symbols × references) scans. Use `Scoping::get_resolved_reference_ids(symbol)`, or index once into a `HashMap`.

## Scope the work

- **One pass, many consumers.** Feed every collector from one walk, or use the oxc linter's flat `AstNodes` loop bucketed by `AstType` (see `references/oxc-toolkit.md`). N checks should never mean N walks.
- **Query a window, not the file.** Zed uses `set_containing_byte_range` over a window of about 16 KiB around the point of interest, plus `set_max_start_depth`. Do the same for cursor or point lookups.
- **Cache per-file facts** keyed by `(content hash, grammar or SourceType + options, tool version)`. Never key on `NodeId`, `SymbolId` or pointers, which die with the tree or arena. Store owned facts (strings + byte spans).
- ast-grep `Position::column` scans the line backwards on each call. On minified files, compute columns through a `LineIndex` instead.

## Parallelism

- Run `par_iter` over files, one parser and one allocator per worker. `Parser` is `Send + Sync`, but `parse` takes `&mut self`, so share by thread, not by lock.
- Group files by language so each worker's `set_language` rarely changes.
- Don't let a single slow file hold the pool: every per-file task has its own deadline.
- Don't cache parsed trees across requests unless you measure a real reuse rate. Trees are big and the source may change.

Next: which house helper already does this → `references/octocode-engine-map.md`.
