# Pi Built-in Tool Policy

Octocode intentionally changes Pi 0.84.4's built-in tool surface to reduce duplicate routes and strengthen mutation safety.

## Pi's installed built-ins

The installed `@earendil-works/pi-coding-agent` exports seven built-in tools:

| Built-in | Pi role | Normally enabled by default? |
|---|---|---:|
| `read` | Read files/images | Yes |
| `bash` | Execute shell commands | Yes |
| `edit` | Exact text replacement | Yes |
| `write` | Create/overwrite files | Yes |
| `grep` | Search file contents | No; available/configurable |
| `find` | Find paths by glob | No; available/configurable |
| `ls` | List directories | No; available/configurable |

Pi settings can change the initial selection through `defaultTools`, `tools`, `excludeTools`, or `noTools`; the table describes the package default.
The Pi extension supplies its public palette and applies its disabled-name policy for
direct-extension hosts and session resets. Native `octocode-agent` has its own tool catalog
and does not configure Pi built-ins.

## Octocode decision matrix

| Pi built-in | Decision | Octocode owner |
|---|---|---|
| `read` | Remove | `MCPTool` → `localGetFileContent`; `readMedia` for visual/audio/video perception |
| `grep` | Remove | `MCPTool` → `localSearch` with `operation:"text"` |
| `find` | Remove | `MCPTool` → `localSearch` with `operation:"files"` |
| `ls` | Remove | `MCPTool` → `localSearch` with `operation:"tree"` |
| `edit` | Remove | `file({type:"edit"})` |
| `write` | Remove | `file({type:"write"})` |
| `bash` | Override | Guarded Octocode `bash` |

`file({type:"delete"})` provides a structured deletion operation. The public palette has one
file-mutation route and one schema cost.

## Why

1. **One research route.** Octocode local tools add pagination, minification, AST/LSP routing, metadata search, and a consistent MCP contract. Native `read`/`grep`/`find`/`ls` duplicate weaker paths.
2. **One file-mutation route.** Agents choose an effect (`edit`, `write`, or `delete`) inside one schema instead of choosing between overlapping tool names.
3. **Safer edits.** Exact replacement is the default; normalized and line-range matching are opt-in. Preflight includes stale-read checks and commit includes a lost-update comparison.
4. **Safer writes.** Writes use the shared path guard and atomic temp-file rename, preserve executable modes on overwrite, create parent directories, and refresh read state.
5. **Safer deletes.** Delete accepts only files or symbolic links, rejects directories, snapshots metadata during preflight, and rechecks it under the per-file mutation queue before unlinking.
6. **Truthful batches.** Duplicate paths are rejected and every mixed operation is preflighted before the first side effect. Execution remains ordered and non-transactional after preflight: a later runtime race can fail while prior completed effects remain.

## Agent routing

| Job | Call |
|---|---|
| Targeted existing-file change | `file({queries:[{type:"edit", path, edits, reasoning}]})` |
| New file or intentional full rewrite | `file({queries:[{type:"write", path, content, reasoning}]})` |
| Explicitly scoped file/symlink removal | `file({queries:[{type:"delete", path, reasoning}]})` |
| Builds, tests, package commands, mechanical work | `bash` |

Read existing files before edit or delete. Prefer exact `oldText`; use `matchMode:"normalized"` only for representation drift and `lineRange` only with freshly read line numbers. Do not use shell redirection, `sed`, or `rm` for ordinary mutations when `file` is available.

## Path and concurrency policy

Structured mutations and detected bash write targets are limited to:

- the current working directory;
- the home directory;
- the OS temporary directory; and
- configured `ALLOWED_PATHS` roots.

`file` serializes each path with `withFileMutationQueue`. Peer work is surfaced through the shared mutation gate. Delete deliberately does not support directories; recursive removal remains an explicitly authorized shell workflow.

## Implementation map

| Concern | Location |
|---|---|
| Built-in disabled/override lists | `src/constants.ts` |
| Pi-host active-tool enforcement | `src/index.ts` |
| Unified public contract and delete operation | `src/tools/file-tool.ts` |
| Exact-edit engine and diff generation | `src/tools/edit-tool.ts` |
| Atomic-write engine | `src/tools/write-tool.ts` |
| Read-state and mutation queue | `src/tools/file-state.ts` |
| Path policy | `src/tools/path-guard.ts` |
| Guarded shell | `src/tools/bash-tool.ts` |
| Plan-mode effects | `src/tools/plan-mode.ts` |

The edit/write modules remain internal engines; only `file` is registered publicly.

## Verification contract

- The public palette contains `file` and `bash`, never `edit` or `write`.
- Direct Pi-extension sessions suppress replaced Pi built-ins through the extension policy.
- `DISABLED_BUILTIN_TOOL_NAMES` contains `read`, `edit`, `write`, `grep`, `find`, and `ls`.
- `OVERRIDDEN_BUILTIN_TOOL_NAMES` contains only `bash`.
- Mixed batches preflight before mutation.
- Delete rejects directories, missing targets, changed targets, and paths outside allowed roots.
- Existing edit/write engine tests remain green alongside unified `file` integration tests.
