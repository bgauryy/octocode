# Octocode vs bare POSIX baseline — local React checkout

`ROOT` = the frozen local React checkout (see README: commit-pinned; default
`packages/octocode-benchmark/context/react`). All questions are LOCAL —
no network. Arm A may use only `grep/find/cat/ls/wc/awk/sed`; Arm B only
`node packages/octocode/out/octocode.js tools …`.

**Reporting contract (every question):** log every command as
`{cmd, exit, bytes-of-output, ms}` and state total bytes + call count per
question. Counting questions must lift result caps — a silently truncated
count scores 0.

## Q1

Across `$ROOT/packages/react-dom/src`, count CALL EXPRESSIONS of the exact shape
`useState(<arg>)` (real AST calls — not comments, strings, or longer
identifiers). Report the count and the top 3 files by hits.

## Q2

In `$ROOT/packages/react-is/src`, report (a) the count of exported function
declarations (with a body) and (b) the count of non-exported function
declarations.

## Q3

Across `$ROOT/packages/react-reconciler/src` (excluding `__tests__`), count
`await` expressions that occur INSIDE a `try { … }` block.

## Q4

`createFiberFromText` is declared in
`$ROOT/packages/react-reconciler/src/ReactFiber.js`. Report every real
REFERENCE site (semantic usage, excluding the declaration itself) as
`file:line` across the repo.

## Q5

In `$ROOT/packages/react-reconciler/src/ReactFiberWorkLoop.js`, report the
count of top-level function declarations, plus the name and line of the first
and last one.

## Q6

Considering `$ROOT/packages/react-is` as a whole package (public surface =
what `index.js` exposes), report how many of its exported symbols are never
used anywhere outside the package itself, and list them (or state that there
are none). Then PROVE the verdict for one symbol of your choice: show the
whole-repo evidence (search or semantic references) that its only occurrences
are its definition/re-export sites — an unproven list caps at half credit.

## Q7

Report the single largest `.js` file (path + exact byte size) under
`$ROOT/packages/react-reconciler/src`, excluding `__tests__`.

## Q8

In `$ROOT/packages/react-reconciler/src/ReactFiberWorkLoop.js`, the function
`scheduleUpdateOnFiber`: report (a) its definition `file:line`, and (b) every
semantic CALLER across the repo (excluding `__tests__`) as
`callerFunctionName — file:line`. Raw text matches of the identifier are not
proof — a match inside a comment, string, or re-export does not count; each
reported site must be a real call.

## Q9

Count the FILES (not lines) under `$ROOT/packages/react-reconciler/src` that
contain the token `__DEV__`, excluding `__tests__`.

## Q10

For `$ROOT/packages/react-reconciler/src/ReactFiberBeginWork.js`, report the
total top-level symbol count and the `name:line` of the first and last
top-level function. Your process is graded on cost as well as correctness.
