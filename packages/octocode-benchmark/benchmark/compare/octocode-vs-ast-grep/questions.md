# Octocode vs ast-grep — Questions

**Corpus:** the frozen local React checkout (see README — clone to
`packages/octocode-benchmark/context/react`, commit `9ceb1e7d`). `CORPUS` =
absolute path of that checkout. All questions run against `$CORPUS` — never
against the octocode repo itself (a live repo drifts; a pinned corpus doesn't).
Unless a question says otherwise, scope to `*.js` files only (React is
Flow-typed `.js`), and state the scope you actually used.

**Reporting contract (every question):** alongside the answer, log every
command as `{cmd, exit, bytes-of-output, ms}` and state the total bytes and
call count you spent on the question. Counting questions must lift result caps
(`maxFiles` / `maxMatchesPerFile` / `itemsPerPage` / equivalents) — silently
truncated counts score 0. Line-number sets must be normalized to 1-based
before comparison.

## Q1 — call-shape count (parity)

In `$CORPUS/packages/react-reconciler/src`, count call expressions of the
shape `useState(<args>)`. Report the count.

## Q2 — member-call sites (parity, exact set)

In `$CORPUS/packages/react/src`, find every call of the shape
`<obj>.push(<args>)`. Report the count and the full sorted `file:line` list.

## Q3 — reconcile a divergence (analysis)

Run the same `<obj>.push(<args>)` search from Q2 against
`$CORPUS/packages/react-reconciler/src` with BOTH your primary tool and any
cross-check you can construct. If two methods disagree on the count, do not
average or pick one: attribute the difference to specific `file:line`
entries and explain the mechanism. Report the count(s) you stand behind and
the attribution.

## Q4 — relational rule (parity)

In `$CORPUS/packages/react-devtools-shared/src`, find every `await`
expression inside a `try { … }` block (any depth). Report the count and the
file(s).

## Q5 — whole-corpus census + wall-clock (scale)

Across ALL of `$CORPUS/packages` (`*.js`), count call expressions of the
shape `<fn>(<args>)`. Report the count and the wall-clock time of the single
counting command (cold process). If your count differs from a cross-check by
more than 0.5%, attribute it.

## Q6 — cross-file callers (identity, beyond text)

`scheduleUpdateOnFiber` is defined in
`$CORPUS/packages/react-reconciler/src/ReactFiberWorkLoop.js`. Report every
file that references it OUTSIDE its defining file, with a per-file reference
count. Name-collision text hits (comments, strings, same-named locals) must
not be counted — state how your method guarantees that.

## Q7 — dead exports (reachability)

In `$CORPUS/packages/scheduler`, list exported symbols that are candidates
for being dead (unreferenced from the package's entrypoints), and verify at
least two candidates by an independent method before claiming them. Report
the candidate list, the two verifications, and your confidence.

## Q8 — outline surface (condensed reading)

Produce a symbol-level outline of
`$CORPUS/packages/react-reconciler/src/ReactFiberWorkLoop.js` (top-level
functions/exports with line numbers). Report: (a) the number of top-level
function symbols, (b) the byte size of the outline your toolchain produced,
(c) the full file's byte size, and (d) the ratio.

## Q9 — bounded read (byte cost of one function)

Read ONLY the body of `scheduleUpdateOnFiber` from
`$CORPUS/packages/react-reconciler/src/ReactFiberWorkLoop.js` — no more than
the function plus a few context lines. Report the exact bytes your toolchain
returned for that read and the `startLine-endLine` you got.

## Q10 — composite flow (find → outline → read)

Starting from only the symbol name `flushSyncWork`: (a) find the file in
`$CORPUS/packages` that defines it, (b) outline that file, (c) read just the
`flushSyncWork` definition. Report the defining `file:line`, the total number
of tool calls, and the total bytes of tool output consumed across all three
steps.
