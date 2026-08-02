# Octocode MCP vs CLI — Questions

`ROOT` = absolute path of this repo's checkout. Both arms answer the same 10
questions; each names the tools it is designed to exercise (the union covers
all 15 active tools). Frozen once any solver starts. Never contains answers.

**Reporting contract (every question, both arms):** log every tool invocation
as `{tool, args-bytes, result-bytes, ms}` and report per-question totals:
calls, result bytes entered into context, and (CLI arm) raw stdout bytes vs
bytes actually read. The harness reports authoritative agent tokens per arm
(input/output/cache split) — see README § Measuring token usage.

## Q1 — npmSearch, ghViewRepoStructure

For the npm package `fastify`: (a) its GitHub source repo (resolved via the npm
surface, not memory); (b) the top-level directories of that repo's default
branch, and which one holds the request-lifecycle implementation.

## Q2 — ghSearchRepos, ghSearchCode

(a) Find the most-starred GitHub repository whose primary topic/purpose is the
`zod` schema-validation library. (b) In that repo, locate the file that defines
`ZodString` and give one `file:line` anchor for its class/type declaration.

## Q3 — ghGetFileContent

In `microsoft/TypeScript` `src/compiler/checker.ts`, read ONLY the function
`isTypeComparableTo`: report its parameters, the function it delegates to, and
the bytes of content you transferred versus the full file size.

## Q4 — ghSearchPullRequests

For the most recently merged PR in `fastify/fastify` that changes `lib/`: (a)
files changed and net line delta; (b) the source (`lib/`) vs test/docs split;
(c) the single source file with the largest combined diff; (d) what behavior or
bug motivated the change, grounded in the diff hunks or linked issue (cite it),
not the PR title.

## Q5 — ghSearchIssues, ghSearchCommits

In `pallets/flask`, find one closed issue about `add_url_rule` behavior and the
commit (sha + one-line summary) that addressed it, verified via commit history
of the file that defines `add_url_rule`.

## Q6 — ghCloneRepo, localSearchCode (structural)

Materialize `colinhacks/zod`'s `src/` locally and report the AST call-expression
count of `z.lazy(<args>)` (structural pattern, not text), with the top 2 files
by hits.

## Q7 — localViewStructure, localFindFiles

In `$ROOT/packages/octocode-tools-core`: (a) map the top 2 levels of `src/`
(directories only) and name where GitHub query building lives; (b) find the 3
largest `*.ts` files under `src/tools/` by size, with sizes.

## Q8 — localGetFileContent

Produce a symbols outline of
`$ROOT/packages/octocode-tools-core/src/tools/toolNames.ts`: list every exported
symbol with its line, and report outline bytes versus full-file bytes.

## Q9 — lspGetSemantics

In `$ROOT/packages/octocode-tools-core`, take the exported function
`buildDirectToolCommandPatterns` (find its real definition anchor first) and
report: its definition `file:line`, every reference `file:line` across the
package, and one caller with the calling line.

## Q10 — localFindDeadCode, localSearchCode

In `$ROOT/packages/octocode-tools-core`, name 3 likely-dead exported symbols
(unreferenced exports) with `file:line`, and prove one of them dead by showing a
whole-package search for its identifier returns only the definition site.
