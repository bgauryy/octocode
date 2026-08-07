# Octocode arm primer

Inject as the `octocode` runner's only primer. Run every research call through
`npx octocode tools …` (no MCP, no gh). Complete catalog:

| Tool | Use |
|---|---|
| `ghSearchCode` | Search GitHub code contents or paths. |
| `ghGetFileContent` | Read an exact GitHub file, region, or match. |
| `ghViewRepoStructure` | Browse a GitHub repository tree. |
| `ghSearchRepos` | Discover repositories. |
| `ghSearchPullRequests` | Search or inspect pull requests, files, and diffs. |
| `ghSearchIssues` | Search or inspect issues and comments. |
| `ghSearchCommits` | Inspect commit history, paths, or ranges. |
| `ghListReleases` | List releases when `ENABLE_RELEASES=true`. |
| `ghSearchDiscussions` | Search discussions when `ENABLE_DISCUSSIONS=true`. |
| `ghCloneRepo` | Materialize a repo or sparse subtree for repeated local analysis. |
| `npmSearch` | Resolve an npm package and its source repository. |
| `localSearchCode` | Search cloned/local text, regex, or AST structure. |
| `localGetFileContent` | Read an exact cloned/local file or region. |
| `localViewStructure` | Browse a cloned/local directory tree. |
| `localFindFiles` | Find cloned/local files by path metadata. |
| `localFindDeadCode` | Find likely-unreferenced exports in a cloned repository. |
| `lspGetSemantics` | Definitions, references, callers/callees, types, symbols in cloned code. |

`npx octocode tools --json` lists the enabled catalog; `… <name> --scheme` gives a schema
(those calls count). Typical forms:

```bash
npx octocode tools ghSearchCode --queries '{"owner":"OWNER","repo":"REPO","keywords":["TERM"]}'
npx octocode tools ghGetFileContent --queries '{"owner":"OWNER","repo":"REPO","path":"PATH","branch":"SHA","matchString":"SYMBOL"}'
npx octocode tools ghViewRepoStructure --queries '{"owner":"OWNER","repo":"REPO","branch":"SHA","path":"PATH"}'
npx octocode tools ghSearchPullRequests --queries '{"owner":"OWNER","repo":"REPO","prNumber":123}'
```

Prefer targeted remote reads; use `ghCloneRepo` + local search/LSP only when the question
needs repeated reads, structural matching, or semantics. A disabled opt-in tool is
unavailable, not a failed requirement.
