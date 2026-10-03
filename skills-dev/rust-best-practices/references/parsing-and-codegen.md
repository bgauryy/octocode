# Parsing & code transformation — tree-sitter, offsets, safe rewrites

Load when parsing source (tree-sitter, oxc, syn), running structural queries, or rewriting code by byte range.

## Parser lifecycle
- **Reuse the parser** per worker (`thread_local! { RefCell<Parser> }`), `reset()` + `set_language()` per file — never `Parser::new()` in a hot loop (see `references/performance-and-memory.md`).
- **Compile queries once** and share (`OnceLock`/`LazyLock` → `Arc<Query>`); allocate the `QueryCursor` per call (cheap) or thread-local it.
- **Full re-parse is fine for file-at-a-time / batch** analysis; incremental (`Tree::edit`) only pays off for interactive editing of the same buffer. Don't add incremental complexity for batch tools.
- **Contain parser panics across FFI**: wrap third-party parser calls (oxc, C grammars) in `catch_unwind` (and a deep-stack runner for deeply-nested input) so an ICE can't unwind across a `cdylib`/N-API boundary (`references/safety-and-ffi.md`).

## Query & predicate safety
- Validate query shape up front; reject unknown fields (`#[serde(deny_unknown_fields)]`) and unknown node kinds against the grammar's symbol table.
- **Reject predicates you don't evaluate.** If a query carries a custom/general predicate your engine can't run, fail the query — never silently ignore it, or you'll drop/keep the wrong nodes.
- Bound the work: recursion depth cap, match-count limit, backtracking budget, and a wall-clock deadline (tree-sitter progress callback) so a pathological pattern/file can't hang a worker.

## Offset handling (the silent corrupter)
- Centralize byte↔line/column conversion in one `LineIndex`; don't hand-roll per call site.
- Decide the **column unit deliberately**: LSP and JS tooling use **UTF-16 code units**, not bytes or scalar values — mismatches misplace edits on any non-ASCII line.
- **Snap to char boundaries**: an offset landing mid-multibyte-char must floor to the boundary, not collapse to 0. Use `.get(range)` (returns `Option`) over raw slice indexing (`&bytes[a..b]` panics on a bad/misaligned range). Test with emoji / surrogate pairs / CRLF / trailing newline.

## Rewriting code safely
- **Apply non-overlapping edits back-to-front** (descending start byte) so earlier offsets stay valid as you splice; **reject overlapping edits** outright.
- **Verify before you write**: at apply time assert `source[start..end] == expected_old_text` (catches a stale match against changed source); require the output be valid UTF-8; cap patch/file size.
- **Re-parse the result to validate syntax.** Byte-splicing cleanly does *not* mean the output parses — re-parse the staged text and reject if it introduces new parse/ERROR nodes vs the original. A "matches-remaining == 0" postcondition is not a validity check.
- Make apply transactional: content-hash the source+query snapshot, re-check file hashes against disk before commit, journal for rollback. A partial multi-file rewrite must be recoverable.

Next: for parser reuse/allocation cost, load `references/performance-and-memory.md`; for untrusted-input bounds, `references/safety-and-ffi.md`; for the FFI panic boundary, `references/safety-and-ffi.md`.

For handles, walking, queries, octo/oxc/ast-grep specifics and the engine defect list, use the `ast-best-practices` skill.
