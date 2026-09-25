# Testing tree code

Load when adding or changing a parser call, walker, matcher, position conversion or rewrite, and before claiming one works. Why: tree code passes the tests you wrote with ASCII, LF and valid syntax, then breaks on the first emoji, CRLF, BOM, deep nesting or broken file a user feeds it.

## Required fixture classes

Generate them with `scripts/adversarial-fixtures.mjs` (run `--help` for usage). Each class catches a known failure:

| Fixture | Catches | Source of the failure |
|---|---|---|
| emoji and astral characters (surrogate pairs) | byte vs UTF-16 vs char column mix-ups | every stack stores bytes; LSP uses UTF-16; ast-grep uses a char count |
| CRLF line endings | CR counted as part of the line, or not | tree-sitter rows count `\n` only |
| leading BOM | row-0 columns off by 3 bytes or 1 UTF-16 unit | tree-sitter skips the BOM as whitespace but keeps the bytes in offsets |
| no final newline | last-line slicing and clamping | `LineIndex` end handling |
| broken syntax | `ERROR`/`MISSING` handling, metavariables bound to `MISSING` | recovery always returns a tree |
| deep nesting (10k–100k brackets) | stack overflow in recursive walkers or oxc | oxc and many walkers recurse with no guard |
| one huge line (≥1 MB) | O(line) column scans, match-limit loss, size caps | ast-grep `Position::column` scans backwards |
| invalid UTF-8 bytes | panics on `utf8_text` or `&str` conversion | the lexer emits `ERROR` for bad bytes and doesn't fail |
| NUL byte mid-file | TS external scanner treats NUL as EOF and inserts automatic semicolons | tree-sitter-typescript scanner |
| many matches (>65k in progress) | `did_exceed_match_limit` handling | match states are dropped mid-run |

## Test techniques

- **Tree snapshots.** Snapshot `node.to_sexp()` for tree-sitter, `Pattern::dump` for ast-grep and `insta::assert_snapshot!` for oxc diagnostics. Review the snapshot diff; never bless it blindly.
- **Position round-trip.** For every fixture, byte → (line, UTF-16 col) → byte must give back the original offset at char boundaries. Test through the shared `LineIndex`, not a copy of it.
- **Incremental equals fresh.** If you use `Tree::edit`: apply a random `InputEdit`, reparse incrementally, and assert that `to_sexp` equals a fresh parse (the proptest version of `tree-sitter fuzz`).
- **Differential checks.**
  - Optimized path vs naive path on the same input (bucketed vs full walk, prefiltered vs unfiltered). The oxc linter does this in debug builds.
  - Rewrite → reparse → error count not greater than before.
  - oxc parse → codegen → reparse → `ContentEq` for transforms.
- **Limits are behavior.** Assert that the deadline, size cap, match cap and depth cap each produce the documented limit diagnostic. Never allow a panic, a hang or a silently short result.
- **Real tool path.** After engine changes, run the built CLI or MCP tool on the fixtures (see `references/octocode-engine-map.md`), not only unit tests.

## Fuzzing (optional, for parsers and matchers on untrusted input)

`cargo fuzz` targets: parse → walk → match over arbitrary bytes, with the deadline set. Seed the corpus with the fixture classes above. Any panic, abort, timeout or out-of-memory is a defect.

Next: the house test and build commands → `references/octocode-engine-map.md`.
