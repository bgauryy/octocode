# tree-sitter queries

Load when writing or running a tree-sitter `Query`. Why: an uncapped query can silently lose matches mid-run, and predicates you don't evaluate can be silently ignored.

## tree-sitter `Query`

- **Compile once, share everywhere.** `Query` is `Send + Sync`. Cache it per `(Language, &'static str)` in a `OnceLock<RwLock<HashMap<_, Arc<Query>>>>` (house: `signatures/extractor.rs::cached_query`). Compiling costs far more than running.
- **One `QueryCursor` per call or per worker.** Bound every run:
  - `set_match_limit(n)` caps **in-progress** match states, not total results. When the pool fills up, the engine **drops the earliest still-in-progress match mid-run**. Matches already yielded stay valid, but the missing ones can sit anywhere, including earlier in the document, so results aren't "complete up to a point". After the loop, `if cursor.did_exceed_match_limit()` return *incomplete*. Never return a partial result as if it were complete.
  - Pick the limit on purpose. Zed and nvim use 64, Helix uses 256, and the house value is 65 536 (the documented maximum). Low limits lose matches in wide or deep files; high limits cost memory.
  - `QueryCursorOptions::new().progress_callback(..)` returns `ControlFlow::Break` at the deadline.
  - `set_byte_range(r)` limits the search window. `set_containing_byte_range(r)` requires matches to lie fully inside. `set_max_start_depth(Some(0))` anchors the pattern at the node you query.
- **Predicates.**
  - The Rust binding evaluates all ten text predicates: `eq?`, `not-eq?`, `any-eq?`, `any-not-eq?`, `match?`, `not-match?`, `any-match?`, `any-not-match?`, `any-of?`, `not-any-of?`. The regex is `regex::bytes`, so there is no lookaround and no backreferences.
  - `#set!` goes to `property_settings`. `#is?` and `#is-not?` go to `property_predicates`. `#select-adjacent!`, `#strip!` and every other directive go to `general_predicates(i)`. **Reject any predicate or directive you don't evaluate.** Ignoring one keeps the wrong nodes, and rejecting them also rejects most nvim and helix query files.
  - Predicates run **after** the C engine yields a match, so `#match?` never reduces traversal cost.
- **`matches` vs `captures`.** `matches` groups results per pattern and is not strictly in document order. `captures` streams captures in document order, which suits highlighting. Both are `StreamingIterator`s, so call `while let Some(m) = it.next()`, not `for`.
- Resolve `capture_index_for_name("body")` once, outside the loop. Use `disable_capture` and `disable_pattern` to drop work you won't read, and call `deep_clone()` first on a shared query.
- **Query syntax worth knowing.**
  - Quantifiers `+ * ?` work on groups and alternations `[...]`.
  - The anchor `.` pins first child, last child or immediate sibling, and **ignores anonymous nodes**.
  - `!field` asserts the field is absent. `(_)` is any named node; `_` is any node.
  - `(supertype/subtype)` is allowed.
  - Costly shapes: non-local, unrooted patterns like `((a) (b))`, root-level `(_)` and huge alternations. Check them with `is_pattern_rooted`/`is_pattern_non_local`.
- **Compile errors.** `QueryErrorKind::NodeType` on a query that used to work usually means the grammar version changed. `Structure` ("impossible pattern") means the pattern can never match under this grammar.

Next: ast-grep rules or octo patterns → `references/patterns-and-rules.md`; turning matches into edits → `references/editing.md`.
