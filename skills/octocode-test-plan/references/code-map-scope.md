Load when mapping what a diff, branch or PR affects, the documented claims about a feature, or the tests that already cover it. Output mode and query habits are in [code-map recipes](code-map.md).

# Map change scope, claims and existing tests

## Change scope

For a diff, branch or PR, list changed files with `git diff --name-only <base>...HEAD` (a PR: `ghGetHistoryItem` `operation:"pullRequest"`, `sections:["files"]`). Then:

| Need | Call |
|---|---|
| Build the graph once per scope | `graphBuild` `path:<smallest directory holding the change and its consumers>`; include its `.octocode/graph` cache write in the authorized scope |
| What depends on the change | `graphQuery` `operation:"impact"`, `changed:[paths]` or `since:"<base>"` |
| Files that change together, often a test and its subject | `graphQuery` `operation:"cochange"`, `target:<path>` |
| Callers of a changed symbol | `lspSearch` `callers`; graph rows are syntax candidates until LSP confirms them |

When the graph tools are not listed, use `lspSearch references` per changed export and name the gap.

## Documented claims

| Need | Call |
|---|---|
| Docs that mention the feature | `localSearch` the feature's public name over `docs`, `README*`, `*.md`, `view:"files"` |
| One section | `localFetch` `matchString:["## Heading"]`, `block:true`: the whole section, subsections included |
| Help text, schema descriptions, error messages | `localSearch` for the user-facing string; these are contracts too |

Record each claim with its source line. A claim the code contradicts goes under the plan's Findings, with both anchors, when the deciding contract is known (a test, schema or owner states which side is right); when it is not, it goes under Gaps.

## Existing tests

| Need | Call |
|---|---|
| Test files in scope | `structureSearch` `operation:"files"`, `path:<target>`, `testCode:"only"`, `view:"paths"` |
| Tests that name a symbol | `localSearch` `matchString:["<symbol>"]`, `testCode:"only"`, `view:"files"` |
| Code no test mentions | Search the symbol with `testCode:"only"`; inspect scan coverage and confirm with `lspSearch references` before claiming a gap |
| Test outline: names and groups | `localFetch` `view:"symbols"` on the test file |
| Fixtures, mocks, helpers | `localSearch` for the fixture name or `mock`, `stub`, `fixture` with `testCode:"only"` |

`testCode` covers test files and directories by convention plus Rust `#[cfg(test)]` items; it misses self-test modes inside a program and test helpers with ordinary names, so also check `--help` for `--self-test` or `--check` and search for the runner's import (`node:test`, `vitest`, `pytest`). Confirm command syntax from the [command sources](code-map.md#layout-and-entry-points). List/collection modes such as `cargo test -- --list`, `pytest --collect-only -q`, `vitest list` and `go test -list .` can build code, run initialization or load project plugins (`vitest list` runs the test files before Vitest 5 or with `--no-static-parse`; Vitest 5 and later parse them statically); use them only when those effects are authorized, and record the result. They do not establish that the cases passed.
