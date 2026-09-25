# Editing and rewriting

Load when mutating source through a tree: rewrite tools, fixers, codemods, incremental reparse. Why: a splice that looks clean can still produce code that doesn't parse, a stale match can corrupt a file that changed on disk, and a regenerated AST can wipe formatting and comments.

## Default: span splicing plus validation

1. **Collect** `(start_byte, end_byte, replacement)` edits from one parse of one snapshot.
2. **Sort** by `(start, end)`. **Reject** overlapping or identical ranges outright; don't try to merge them.
3. **Apply back to front** (descending start), so earlier offsets stay valid.
4. **Guard every splice:** `src.get(start..end) == Some(expected_old)`. Otherwise the file drifted, so abort.
5. **Reparse** the output and count `ERROR` + `MISSING` nodes. Reject if the count after is greater than the count before. "Zero matches remain" is not a validity check.
6. **Make it transactional** for multi-file changes: take a content hash at preview time, recheck the hashes against disk at apply time, and keep a journal so rollback is possible.

House implementation: `runtime/src/tools/ast_rewrite/mod.rs` (`prepare_matches`, `apply_edits`, `check_syntax_regression`, expected-hash gates, journal and lock). Extend it; don't fork it.

## ast-grep specifics

- `Root::edit` and `replace` splice the string (O(n)) and then reparse **incrementally** on every call. For N edits, build `Edit`s with `NodeMatch::make_edit` (or fixer `get_replaced_range` + `generate_replacement`), then run the protocol above and reparse once.
- The replaced range can differ from the matched node's range. `expandStart`/`expandEnd` widen it, and under Smart strictness trailing nodes can make it **shorter**. Report the range you actually spliced.
- ast-grep has no deadline of its own. Cap the input size and match count first (house: 1 MB, 100k matches).

## oxc specifics

| Approach | Keeps formatting and comments | Use when |
|---|---|---|
| `Span` text splicing | yes | targeted edits, minimal diffs (the default) |
| `VisitMut`/`Traverse` + `oxc_codegen::Codegen` | **no**: all formatting is lost. Only statement-level, JSDoc, annotation and legal comments are printed; expression-level comments are dropped | whole-file transforms or minification |

Build replacement text from the original source slices, so a replacement doesn't re-escape or normalize the user's code.

For `Traverse`, call `traverse_mut(&mut t, &alloc, &mut program, scoping, state)`. It needs a `Scoping` built beforehand (`SemanticBuilder::new().build(&program).semantic.into_scoping()`) and returns the updated one. Build new nodes with `ctx.ast`.

## Incremental reparse (tree-sitter)

This is only worth it when the same buffer is edited interactively. For batch or one-shot tools, reparse the whole file.
1. Call `tree.edit(&InputEdit { start_byte, old_end_byte, new_end_byte, start_position, old_end_position, new_end_position })`. Bytes **and** Points must both be correct.
2. Call `parser.parse(new_src, Some(&tree))`.
3. Call `old.changed_ranges(&new)` and re-query only those ranges with `set_byte_range`.

Any `Node` you kept is invalid after step 1 unless you shift it with `node.edit(&e)`.

Next: making it fast → `references/efficiency.md`.
