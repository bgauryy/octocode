# Octocode vs ast-grep — Questions

**Suite version:** 2.

**Corpus:** the frozen local React checkout (see README — clone to
`packages/octocode-benchmark/context/react`, commit
`9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d`). `CORPUS` = absolute path of that
checkout. All questions run against `$CORPUS` — never against the octocode repo
itself (a live repo drifts; a pinned corpus doesn't). Unless a question says
otherwise, scope to `*.js` files only (React is Flow-typed `.js`), and state
the scope you actually used.

**Sealed-arm contract:** a solver is assigned exactly one arm for the complete
run of all ten questions.

- Arm A may use only the `ast-grep` CLI on local files.
- Arm B may use only
  `node packages/octocode/out/octocode.js` local-tool surfaces.
- Do not invoke the other arm, generic search/read utilities, or an unspecified
  cross-check. Do not view the other solver's output before sealing yours.
- An independent judge receives both sealed outputs, normalizes their anchors,
  compares them, and reconciles discrepancies from the evidence each arm
  reported. A solver reports only what its assigned arm observed.

**Reporting contract (every question):** alongside the answer, log every
command as `{cmd, exit, bytes-of-output, ms}` and state the total bytes and
call count spent on the question. For every read or result page, report raw
bytes returned and bytes actually read by the solver. Counting questions must
demonstrate that the complete result set was consumed; silently truncated
counts score 0. Normalize line numbers to 1-based before reporting.

## Q1 — call-shape count (parity)

In `$CORPUS/packages/react-reconciler/src`, count call expressions of the
shape `useState(<args>)`. Report the count.

## Q2 — member-call sites (parity, exact set)

In `$CORPUS/packages/react/src`, find every call of the shape
`<obj>.push(<args>)`. Report the count and the full sorted `file:line` list.

## Q3 — reconcile a divergence (analysis)

Run the same `<obj>.push(<args>)` search from Q2 against
`$CORPUS/packages/react-reconciler/src` using only your assigned arm. Report
the count and complete sorted `file:line` anchors. Inspect every returned
match span that appears inconsistent with the requested call shape; report
its span boundaries, relevant source excerpt, and why it is suspicious. Seal
that evidence without estimating or requesting the other arm's result. The
independent judge performs the cross-arm reconciliation.

## Q4 — relational rule (parity)

In `$CORPUS/packages/react-devtools-shared/src`, find every `await`
expression inside a `try { … }` block (any depth). Report the count and the
file(s).

## Q5 — whole-corpus census + wall-clock (scale)

Across ALL of `$CORPUS/packages` (`*.js`), count call expressions of the
shape `<fn>(<args>)`. Report the count and the wall-clock time of the single
counting command (cold process). Report enough completion evidence for the
judge to distinguish a full census from a capped or partial result.

## Q6 — cross-file callers (identity, beyond text)

`scheduleUpdateOnFiber` is defined in
`$CORPUS/packages/react-reconciler/src/ReactFiberWorkLoop.js`. Report every
file that references it OUTSIDE its defining file, with a per-file reference
count. Name-collision text hits (comments, strings, same-named locals) must
not be counted — state how your method guarantees that.

## Q7 — dead exports (reachability)

In `$CORPUS/packages/scheduler`, list exported symbols that are candidates
for being dead (unreferenced from the package's entrypoints), and verify at
least two candidates with a distinct operation available within your assigned
arm before claiming them. Report the candidate list, the two verifications,
and your confidence.

## Q8 — outline surface (condensed reading)

Produce a symbol-level outline of
`$CORPUS/packages/react-reconciler/src/ReactFiberWorkLoop.js` (top-level
function declarations, including exported and non-exported functions, with
line numbers). Report: (a) the complete sorted function list, (b) the total
function count, (c) raw bytes returned and bytes read for every call or page,
and (d) total calls and bytes for the question. Do not assume a fixed byte
target or parser limitation; the judge compares measured cost and completeness
across the sealed arms.

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
