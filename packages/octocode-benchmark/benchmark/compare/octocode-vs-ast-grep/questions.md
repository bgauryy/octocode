# Octocode vs `ast-grep` — Questions (solver-facing) — 10 questions

Run from the repo root. `ROOT` = absolute repo path. Q1–Q6, Q8–Q10 target this
repo; Q7 targets a pinned public repo. For each: give the answer (match count +
sample `file:line` anchors, or the semantic result), the exact pattern/command,
and steps used. Parity questions (Q1–Q5) must report a **match count** so the two
arms can be compared directly.

Arm A uses `ast-grep` (`ast-grep run -p '<pattern>' <path>` or `ast-grep scan`
with a YAML rule). Arm B uses `octocode` (`localSearchCode mode:"structural"`
with `pattern` or `rule`).

## Q1 — metavar pattern parity

Across `$ROOT/packages/octocode-tools-core/src`, find every call of the shape
`JSON.parse($X)`. Report the total match count and the files. (Both arms should
return the identical count.)

## Q2 — function body metavar parity

Across `$ROOT/packages/octocode-tools-core/src/tools`, find every function
declaration of the shape `export function $NAME($$$ARGS) { $$$BODY }`. Report the
match count. (Note: modifiers are part of the node — a pattern without `export`
will not match exported functions.)

## Q3 — relational rule parity

Across `$ROOT/packages/octocode-tools-core/src`, find every `await $X` expression
that occurs **inside** a `try { ... }` block. Use a relational rule
(`ast-grep scan` YAML with `inside`; Octocode `mode:"structural"` with `rule`).
Report the match count.

## Q4 — method-call shape parity

Across `$ROOT/packages/octocode-tools-core/src`, find every method call of the
shape `$OBJ.push($$$ARGS)`. Report the match count and the top 3 files by hits.

## Q5 — cross-language parity

Run the "find all function calls" idea in two languages on this repo: (a) in
TypeScript under `$ROOT/packages/octocode-tools-core/src`, count call
expressions matching `$FN($$$)`; (b) in Rust under
`$ROOT/packages/octocode-engine/src`, count macro/function call shapes you can
express. Report both counts and note any language where one tool lacks grammar
support.

## Q6 — semantic callers (beyond AST shape)

Take the function `isLocalTool` (defined in
`$ROOT/packages/octocode-tools-core/src/tools/toolNames.ts`). List **every place
that actually calls it** across the repo. (a) Give the caller `file:line` set.
(b) Explain why matching the call *shape* `isLocalTool($X)` with a structural
pattern is not the same as resolving the real callers — what can a pure AST match
miss or wrongly include (shadowing, same-named symbols, re-exports)?

## Q7 — structural search on a remote repo

Without manually running `git clone` yourself, find every `useState($$$)` call
expression in `pmndrs/zustand`'s `src/` directory on GitHub. (a) Report the match
count and sample files. (b) State how your toolchain accessed the remote code for
an AST query.

## Q8 — text + structural combined

In `$ROOT/packages/octocode-tools-core/src`, find files that (a) contain the text
`scheme` in their path or content AND (b) contain a structural `z.object($$$)`
call (a Zod object schema). Report the files satisfying **both** and the
structural match count within them.

## Q9 — match → read enclosing function cheaply

Find a structural match of `describe($$$)` (or any real call) in
`$ROOT/packages`, then read the **enclosing function/block** of the first match
as an outline/minified view — not the whole file. (a) Give the match `file:line`.
(b) Report roughly how much content you transferred to read the enclosing context.

## Q10 — beyond the ast-grep grammar set

Pick a format Octocode's structural/minify surface supports (e.g. a `.json`,
`.yaml`, `.md`, or other non-mainstream grammar in this repo). (a) Perform a
structural or symbol-level query over it and report the result. (b) State whether
`ast-grep` has grammar support for that format; if it does not, note that the
task is un-runnable in Arm A.
