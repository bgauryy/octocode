Load to pin an unfamiliar input or map code and tests before designing cases. The CLI prints text by default; add `--json` to read the fields named here. Each query also takes `mainGoal` (the plan's target) and `reasoning` (why this call). Live schemas own exact fields: `octocode schema <tool> --view variants`. [Usage](octocode-usage.md) owns invocation, paging and recovery.

# Map the code for a test plan

Ask small first (paths, outlines, counts), then read the deciding spans. Batch independent queries; reuse observed paths, symbols and lines in the next call.

## Pin by input

| Input | Scope and revision |
|---|---|
| Feature/module | Public surface and direct consumers; `git rev-parse HEAD` |
| Dirty working tree | Identify relevant local diff and untracked inputs alongside the base revision |
| Branch/diff | Changed files and dependents; `git diff <base>...HEAD` and base SHA |
| PR | Head revision (`headCommitSha`); retrieve like `octocode-pr-review` |
| Bug report | Failing path and supplied reproduction, marked unverified |
| Spec/ticket/RFC only | Requirements and document version; code mapping stays a gap until code exists |

## Layout and entry points

| Need | Call |
|---|---|
| Packages, manifests, test and build config | `structureSearch` `operation:"files"`, `include:["**/package.json","**/Cargo.toml","**/pyproject.toml","**/go.mod","**/*.config.*"]`, `view:"paths"` |
| Directory shape of the target | `structureSearch` `operation:"tree"`, `path:<target>`, choose depth for the layout question, or `operation:"files"` for recursive discovery |
| Public surface of one file | `localFetch` `view:"symbols"` |
| Public surface of a directory | `astSearch` `operation:"symbols"`, `path:<target>` |
| Commands, routes, handlers, flags | `localSearch` for the registration term (route method, `command(`, `#[arg`, `argparse`), then `localFetch` with `matchString` and `block:true` |

Test commands come from the manifest scripts, task runner or CI workflow (`.github/workflows/*.yml`). Read the exact line; never guess a runner flag.

## Behavior and identity

| Need | Call |
|---|---|
| One function's body | `localFetch` `matchString:["fn name"]`, `block:true` |
| Who calls a symbol | `lspSearch` `operation:"callers"` at `path`, `symbolName`, `line` from a search hit |
| Every use, including types and re-exports | `lspSearch` `operation:"references"`, `view:"files"` first |
| What it calls | `lspSearch` `operation:"callees"` |
| Inferred type or signature | `lspSearch` `operation:"hover"` |
| Error paths, validation, panics, throws | `localSearch` or `astSearch` for the throw, `Err(`, `raise`, `reject` shape inside the target, then read each site |
| Configuration and environment reads | `localSearch` for the config key or `process.env`, `std::env`, `os.environ` in scope |

When LSP is unavailable for the language, say that caller identity is unverified and keep the AST or text candidates as leads.

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
| One section | `localFetch` `matchString:["## Heading"]`, then a `ranges` read to the next heading |
| Help text, schema descriptions, error messages | `localSearch` for the user-facing string; these are contracts too |

Record each claim with its source line. A claim the code contradicts goes under the plan's Findings, with both anchors.

## Existing tests

| Need | Call |
|---|---|
| Test files in scope | `structureSearch` `operation:"files"`, `path:<target>`, `testCode:"only"`, `view:"paths"` |
| Tests that name a symbol | `localSearch` `matchString:["<symbol>"]`, `testCode:"only"`, `view:"files"` |
| Code no test mentions | Search the symbol with `testCode:"only"`; inspect scan coverage and confirm with `lspSearch references` before claiming a gap |
| Test outline: names and groups | `localFetch` `view:"symbols"` on the test file |
| Fixtures, mocks, helpers | `localSearch` for the fixture name or `mock`, `stub`, `fixture` with `testCode:"only"` |

`testCode` covers test files and directories by convention plus Rust `#[cfg(test)]` items; it misses self-test modes inside a program and test helpers with ordinary names, so also check `--help` for `--self-test` or `--check` and search for the runner's import (`node:test`, `vitest`, `pytest`). To confirm command syntax, read manifests and runner help first. List/collection modes such as `cargo test -- --list`, `pytest --collect-only -q`, `vitest list` and `go test -list .` can build code, run initialization or load project plugins; use them only when those effects are authorized, and record the result. They do not establish that the cases passed.

## Done

The map is complete when each in-scope feature has its entry point, the code path read to its output, its consumers, its documented claims and its current tests or an explicit "no test found in <scanned scope>". Unread scope, unavailable LSP or graph, and unconfirmed commands go to the plan's gaps.
