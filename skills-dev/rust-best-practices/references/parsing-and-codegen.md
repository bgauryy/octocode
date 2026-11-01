Load when generating tool contracts, parsing source (tree-sitter, oxc, syn), running structural queries, or rewriting code by byte range. Why: duplicated schemas and incorrect byte offsets break consumers silently.

# Parsing & code transformation — tree-sitter, offsets, safe rewrites

## Tool contract generation: Octocode
- Author schemas, descriptions, instructions, and limits in `@octocodeai/octocode-core`; `@octocodeai/config` owns generation.
- Build core, then run `node skills-dev/octocode-dev/scripts/dev.mjs contracts:regen` from the repo root. Immediately rebuild native, then affected interfaces with `octocode-dev`.
- Use generated Rust query/result types. Add accessor `impl` blocks when needed; do not mirror the wire format in serde or interface types.
- Native `build.rs` embeds the generated contract in place and checks fingerprints. Regenerate and rebuild on drift; do not bypass the guard.
- Implement each new field/discriminator and declare its native effect in `field-effect-coverage.json`. A declaration alone does not prove behavior.
- Keep open payloads open. Strict input validation follows the authored schema; do not reject extension fields merely for uniformity.
- Verify parity, field effects, and the actual CLI/fresh-MCP path. Generated files are outputs, not manual edit targets.

## Parser lifecycle
- **Consider parser reuse** per worker on a measured hot path. Reset state and change language when the next file requires it; compare against constructing a parser per file before adding thread-local state (see `references/performance-and-memory.md`).
- **Compile queries once** and share (`OnceLock`/`LazyLock` → `Arc<Query>`); allocate the `QueryCursor` per call (cheap) or thread-local it.
- **Full re-parse is fine for file-at-a-time / batch** analysis; incremental (`Tree::edit`) only pays off for interactive editing of the same buffer. Don't add incremental complexity for batch tools.
- Run third-party parsers (oxc, C grammars) on a deep-stack runner for deeply-nested input.

## Query & predicate safety
- Validate query shape up front. Reject unknown fields only in a closed query contract (`#[serde(deny_unknown_fields)]`); validate node kinds against the grammar's symbol table.
- **Reject predicates you don't evaluate.** If a query carries a custom/general predicate your engine can't run, fail the query — never silently ignore it, or you'll drop/keep the wrong nodes.
- Bound the work: recursion depth cap, match-count limit, backtracking budget, and a wall-clock deadline (tree-sitter progress callback) so a pathological pattern/file can't hang a worker.

## Offset handling (the silent corrupter)
- Centralize byte↔line/column conversion in one `LineIndex`; don't hand-roll per call site.
- Decide the **column unit deliberately**: JavaScript string offsets use UTF-16 code units. LSP 3.17+ can negotiate another encoding through `general.positionEncodings` / `capabilities.positionEncoding`; UTF-16 is the default. Convert using the connection's selected encoding, not an assumed byte or character unit. [LSP Position definition](https://github.com/microsoft/vscode-languageserver-node/blob/main/types/src/main.ts#L88).
- **Validate edit boundaries**: reject out-of-range or mid-character edit offsets; silently rounding changes the requested edit. Display-only position conversion may floor to a character boundary if that contract is explicit. Use `str::get(range)` over `&source[a..b]` to check UTF-8 boundaries; byte slices check bounds but do not validate character boundaries. Test emoji, surrogate pairs, CRLF and trailing newline.

## Rewriting code safely
- **Apply non-overlapping edits back-to-front** (descending start byte) so earlier offsets stay valid as you splice; **reject overlapping edits** outright.
- **Verify before you write**: at apply time assert `source[start..end] == expected_old_text` (catches a stale match against changed source); require the output be valid UTF-8; cap patch/file size.
- **Re-parse the result to validate syntax.** Byte-splicing cleanly does *not* mean the output parses — re-parse the staged text and reject if it introduces new parse/ERROR nodes vs the original. A "matches-remaining == 0" postcondition is not a validity check.
- Make apply transactional: content-hash the source+query snapshot, re-check file hashes against disk before commit, journal for rollback. A partial multi-file rewrite must be recoverable.

Next: for parser reuse/allocation cost, load `references/performance-and-memory.md`; for untrusted-input bounds and the FFI panic boundary (`catch_unwind` around parser calls), `references/safety-and-ffi.md`.

For handles, walking, queries, octo/oxc/ast-grep specifics and the engine defect list, use the `ast-best-practices` skill.
