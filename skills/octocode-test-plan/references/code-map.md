Load to pin an unfamiliar input or map code and tests before designing cases. The CLI prints text by default; add `--json` to read the fields named here. Live schemas own exact fields: `octocode schema <tool> --view variants`.

# Map the code for a test plan

Ask small first (paths, outlines, counts), then read the deciding spans. Batch independent queries; reuse observed paths, symbols and lines in the next call. For a diff's impact, documented claims and existing tests, continue with [change scope, claims and existing tests](code-map-scope.md).

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

Test commands come from the manifest scripts, task runner, CI workflow (`.github/workflows/*.yml`) or the runner's help. Read the exact line; never guess a runner flag.

## Behavior and identity

| Need | Call |
|---|---|
| One function's body | `localFetch` `matchString:["fn name"]`, `block:true` |
| Who calls a symbol | `lspSearch` `operation:"callers"` at `path`, `symbolName`; add `line` from a search hit when the file is not the one declaring it once |
| Every use, including types and re-exports | `lspSearch` `operation:"references"`, `view:"files"` first |
| What it calls | `lspSearch` `operation:"callees"` |
| Inferred type or signature | `lspSearch` `operation:"hover"` |
| Error paths, validation, panics, throws | `localSearch` or `astSearch` for the throw, `Err(`, `raise`, `reject` shape inside the target, then read each site |
| Configuration and environment reads | `localSearch` for the config key or `process.env`, `std::env`, `os.environ` in scope |

When LSP is unavailable for the language, say that caller identity is unverified and keep the AST or text candidates as leads.

## Done

The map is complete when each in-scope feature has its entry point, the code path read to its output, its consumers, its documented claims and its current tests or an explicit "no test found in <scanned scope>". Unread scope, unavailable LSP or graph, and unconfirmed commands go to the plan's gaps.
