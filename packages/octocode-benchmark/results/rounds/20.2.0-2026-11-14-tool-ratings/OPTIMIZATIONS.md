# Optimizations and best practices from this round

These recommendations follow the measured questions; they do not claim one tool wins every task.
The scorecard separates the original measurement pin from the rebuilt fix verification.

| Priority | Action | Evidence and acceptance condition |
|---|---|---|
| Fixed | Keep optional recovery calls explicitly named by a diagnostic | The AST result promised an `any` search but workflow filtering removed it. Generic note-reference retention restores the existing call; focused tests protect source/identifier boundaries and required continuations. |
| Fixed and verified | Preserve independent provider records before optimizing output | A URL-union check missed lost reference membership, types and order. The repaired security adapter preserves each provider's selected record and opaque affected metadata. Remove only same-record fix redundancy; another provider cannot supply an omitted value. |
| Fixed and verified | Order page fragments by the actual document | Schema traversal rank is not delivered document order. The shared splitter now retains an ordinal from the actual value walk so indexed declarations precede their dependent hits. |
| Fixed and verified | Disclose oversized indivisible receipts | An unpaged clone receipt still needs an oversized marker when its evidence exceeds the requested window. Keep the complete receipt without creating a replay operation. |
| High | Use a persistent MCP session for repeated local work; measure standalone native CLI separately from its Node launcher | Small-task measurements show a large startup cost in Node CLI compared with MCP. The matched localSearch control returned identical text: native 11.03 ms versus Node 52.07 ms (three pairs). This isolates substantial wrapper cost; it is not a general engine-throughput benchmark. |
| High | Select the narrowest evidence representation that answers the question | Expert rg, source slices and ast-grep projections were smaller in the selected local tasks. Ask for file names, counts or exact ranges when those are the requested facts; preserve complete content when it is requested. |
| High | Reduce repeated metadata in rich graph/LSP answers | Direct locations and narrow Graphify answers are smaller. Preserve source witnesses, uncertainty and every distinct call site; measure a projected representation before changing defaults. |
| High | Compare against explicit competitor caches | Remote gh without an explicit cache flag and a TTL-cached immutable-ref request are different baselines. Report setup, first read, repeat, cache age and selected evidence separately. |
| High | Keep output-page and tool-page state explicit | Tiny windows succeed but can multiply wrapper starts and require nested row-part merging. The long declaration 530 ms CLI result includes ten 3,500-character pages; it is not a default-window latency. Finish response pages first; use realistic page sizes and batch independent queries. |
| Medium | Load only the schemas needed for the current question where the client permits it | The final authenticated 17-tool profile totals 64,208 JSON input-schema characters and 12,171 description characters; these are characters, not token counts. Classification also changes seven other schemas, so totals depend on configuration. `clasify`, `graphQuery` and `astSearch` are the largest input schemas. Measure client context before shortening descriptions or adding output schemas. |
| Medium | Make error guidance executable | graphBuild's file-path refusal is accurate but lacks a runnable directory example. Any change should live in its owning error/contract layer and receive a real CLI/MCP check. |
| Medium | Evaluate classification on a genuinely unread corpus before claiming savings | The small positive cases establish behavior, freshness and billing visibility. They do not establish a cost or recall advantage over direct reading. |

For rg, freeze ignore/hidden/binary scope and choose literal matching when the question is literal.
Count matching lines separately from total occurrences; use path-only or count output only when it
answers the selected question. Sorting can disable parallel search, so include its cost in both arms.
The [ripgrep guide](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md) documents these scope and
output controls.

For lean gh, select the required fields with `--json`, `--jq` or API projections, retain errors and
follow the selected scope to completion. An explicit `gh api --cache` duration is a separate cache
arm, not a request without an explicit cache flag. See the official [formatting guide](https://cli.github.com/manual/gh_help_formatting)
and [API manual](https://cli.github.com/manual/gh_api).

For ast-grep, match the code shape and verify source ranges before counting identity-sensitive
callers. Order dependent metavariable rules with `all`; the official [composite-rule guide](https://ast-grep.github.io/guide/rule-config/composite-rule.html)
explains that evaluation order. Use LSP for resolved identity and retain the language-server coverage
limits rather than treating a syntactic graph edge as proof.

The security findings showed why small output cannot win by itself: the original advisory adapter
dropped ecosystems and package URLs, and a later oracle exposed cross-provider pruning of reference
membership and metadata. The old efficiency comparison is ineligible. Reconstructing the full native
answer proves transport completeness, not source fidelity. Compare every provider's selected record
against a separate raw source before accepting latency or size measurements. Stable advisory IDs do
not make their contents immutable; timestamp source snapshots and record cache age.

For Rust changes, profile the selected boundary in the release profile, preserve borrowed data where
ownership permits, and prefer a narrow generic invariant over fixture-specific branches. The new
recovery filter creates no new calls, uses borrowed text, and preserves existing ordered batches.
The security repair also removes the cross-record temporary decision vector and obsolete range
equivalence helper. Same-record fix suppression borrows the existing events and mutates only after
the decision is computed; no speculative cache or concurrency layer was added.
No new dependency, unsafe code or handwritten wire type was introduced. Cargo profiles affect
optimization and debug behavior; see the [Cargo profile reference](https://doc.rust-lang.org/cargo/reference/profiles.html).
The larger earlier Rust/API audit remains a separately pinned historical report, not a fresh result
of this capability round.

For cache changes, first measure hit/miss state rather than inferring a hit from a faster repeat. The
checked local mutations establish freshness, while the live repository metadata case remains an
explicit gap. Keep complete query/view identity and immutable source identity in any proposed cache
key, preserve cursor snapshot identity, and test edits or upstream changes against the same selected
evidence. These are acceptance conditions for future cache changes, not a claim that this round
implemented a new cache.

Final rebuilt measurements confirm the trade-off: persistent AST MCP 2.90 ms versus ast-grep 8.11 ms, with 1,032 versus 189 output bytes; security MCP 214.13 ms versus parallel lean 453.97 ms, with 7,592 versus 6,456 output characters. The explicit source snapshot is 4.96 ms after a 2,392.03 ms fill; no native cache-hit inference follows from this comparison. See the scorecard for pins, selected scope and receipts.

Very small response targets may force indivisible oversized pieces, so page count need not decrease monotonically as the target changes. The final declaration probes used 5/6 parts at 400 and 11 at 600, preserving all nine declarations and nine dependent hits. Use realistic windows for throughput, and verify complete evidence rather than treating a target as a hard cap.

Use CLI `--json` or MCP for exact numeric comparisons. Readable CLI intentionally rounds fractional values: the observed EPSS percentage 0.20158 prints 0.2016. All three natural security timing observations are retained as descriptive measurements and excluded from strict exact-scalar comparisons. The 15 remaining observations preserve selected-source equivalence. This presentation rule is now documented in the CLI output guide.
