# Optimizations and best practices from this round

These recommendations follow the measured questions; they do not claim one tool wins every task.
The scorecard separates the original measurement pin from the rebuilt fix verification.

| Priority | Action | Evidence and acceptance condition |
|---|---|---|
| Fixed | Keep optional recovery calls explicitly named by a diagnostic | The AST result promised an `any` search but workflow filtering removed it. Generic note-reference retention restores the existing call; focused tests protect source/identifier boundaries and required continuations. |
| High | Use a persistent MCP session for repeated local work; measure standalone native CLI separately from its Node launcher | Small-task measurements show a large startup cost in Node CLI compared with MCP. A native control can distinguish wrapper cost from runtime work; do not call the full difference an engine regression. |
| High | Select the narrowest evidence representation that answers the question | Expert rg, source slices and ast-grep projections were smaller in the selected local tasks. Ask for file names, counts or exact ranges when those are the requested facts; preserve complete content when it is requested. |
| High | Reduce repeated metadata in rich graph/LSP answers | Direct locations and narrow Graphify answers are smaller. Preserve source witnesses, uncertainty and every distinct call site; measure a projected representation before changing defaults. |
| High | Compare against explicit competitor caches | Ordinary remote gh and a TTL-cached immutable-ref request are different baselines. Report setup, first read, repeat, cache age and selected evidence separately. |
| High | Keep output-page and tool-page state explicit | Tiny windows succeed but can multiply wrapper starts and require nested row-part merging. Finish response pages first; use realistic page sizes for normal workflows and reserve tiny windows for correctness stress. |
| Medium | Load only the schemas needed for the current question where the client permits it | The configured17tool input schemas total60805 JSON characters and descriptions12171; these are characters, not token counts. `clasify`, `graphQuery` and `astSearch` are the largest input schemas. Measure client context before shortening descriptions or adding output schemas. |
| Medium | Make error guidance executable | graphBuild's file-path refusal is accurate but lacks a runnable directory example. Any change should live in its owning error/contract layer and receive a real CLI/MCP check. |
| Medium | Evaluate classification on a genuinely unread corpus before claiming savings | The small positive cases establish behavior, freshness and billing visibility. They do not establish a cost or recall advantage over direct reading. |

For rg, freeze ignore/hidden/binary scope and choose literal matching when the question is literal.
Count matching lines separately from total occurrences; use path-only or count output only when it
answers the selected question. Sorting can disable parallel search, so include its cost in both arms.
The [ripgrep guide](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md) documents these scope and
output controls.

For lean gh, select the required fields with `--json`, `--jq` or API projections, retain errors and
follow the selected scope to completion. An explicit `gh api --cache` duration is a separate cache
arm, not an ordinary uncached request. See the official [formatting guide](https://cli.github.com/manual/gh_help_formatting)
and [API manual](https://cli.github.com/manual/gh_api).

For ast-grep, match the code shape and verify source ranges before counting identity-sensitive
callers. Order dependent metavariable rules with `all`; the official [composite-rule guide](https://ast-grep.github.io/guide/rule-config/composite-rule.html)
explains that evaluation order. Use LSP for resolved identity and retain the language-server coverage
limits rather than treating a syntactic graph edge as proof.

For Rust changes, profile the selected boundary in the release profile, preserve borrowed data where
ownership permits, and prefer a narrow generic invariant over fixture-specific branches. The new
recovery filter creates no new calls, uses borrowed text, and preserves existing ordered batches.
No new dependency, unsafe code or generated wire type was introduced. Cargo profiles affect
optimization and debug behavior; see the [Cargo profile reference](https://doc.rust-lang.org/cargo/reference/profiles.html).
The larger earlier Rust/API audit remains a separately pinned historical report, not a fresh result
of this capability round.
