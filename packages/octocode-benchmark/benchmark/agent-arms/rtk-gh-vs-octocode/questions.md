# Root-Cause & Research Flow Questions (solver-facing)

Local repo root for Q1–Q5: `/Users/guybary/Documents/octocode-mcp`
Answer every question with: the answer, evidence anchors (`file:line` or URL/sha),
confidence (high/medium/low), and number of research steps used.

## Q1 — local, root-cause
The octocode CLI command `search --scheme` prints the full OQL schema even on a
machine where the native Rust engine cannot load. Find where the `--scheme` flag
is intercepted (exact file + line) and explain in one sentence why this path
never loads the native engine.

## Q2 — local, root-cause
Symptom: a developer edits CLI source, runs the package build, sees an error,
fixes nothing, and later `packages/octocode/out/octocode.js` still runs OLD code
with no runtime error. Root-cause the build pipeline: what ordering causes a
stale bundle to survive? Cite the exact script definition (file + line).

## Q3 — local, find
Which napi-exported Rust function masks sensitive data in text? Give the
function name, its file and line, and name the directory/module where the
underlying secret-detection logic lives.

## Q4 — local, impact analysis
List every file under `packages/octocode-tools-core/src` that CALLS
`cleanJsonObject(` (invocations; note separately which file DEFINES it).

## Q5 — local, defect localization
Known defect report: "OQL result merge can drop rows because of how the row key
is computed." Locate the function that computes the row key (name, file, line)
and state which fields the key is composed of.

## Q6 — remote (GitHub)
In `pmndrs/zustand` (default branch): which file defines the exported
`createStore`, and which internal function actually implements store creation?
Give file path, both names, and their lines.

## Q7 — remote, root-cause
In `expressjs/express` (default branch): responses get a WEAK ETag by default.
Trace it: (a) where is the default set, and (b) which function compiles the
`etag` setting into an ETag-generating function? Files + lines for both.

## Q8 — remote, find
In `facebook/react`: give the full repo path of the file implementing the
CLIENT-side shim of `useSyncExternalStore` (the one with getSnapshot
change/consistency checks).

## Q9 — remote, history
For the file you found in Q8: find the commit that first introduced it.
Give the sha (short ok), author, and date (month/year precision ok).

## Q10 — remote→local flow
Materialize the repository `sindresorhus/p-limit` locally (clone/fetch with your
toolchain), then answer from the LOCAL copy: (a) how many `.js` files sit at the
repository root, and (b) what does line 1 of `index.js` import? Cite the local
path you materialized to.

> Historical note: in run `20260710T182339Z` Q10 was an npm-metadata question.
> It was excluded from headline scoring afterwards because the baseline arm has
> no package-registry surface (capability parity violation) — see
> `../README.md`, Fairness rules.
