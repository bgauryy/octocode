# Runner tool context

Give a runner `RUNNER.md`, its matchup rules, and **only its assigned arm primer**
before the first research call. Keep the primer identical across questions and
passes, record the tool versions, and never add question-specific advice.

This fixed setup context is not CLI output and is excluded from character totals in both directions (model-in and model-out). Only research-time commands (model-out) and their returned output (model-in), plus the final answer (model-out), are counted.
Any help, catalog, schema, or failed command the runner invokes afterward is a
measured research call.

## Octocode arm

Run every research call through `npx octocode tools …`. The complete catalog is:

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
| `ghCloneRepo` | Materialize a repository or sparse subtree for repeated local analysis. |
| `npmSearch` | Resolve an npm package and its source repository. |
| `localSearchCode` | Search cloned/local text, regex, or AST structure. |
| `localGetFileContent` | Read an exact cloned/local file or region. |
| `localViewStructure` | Browse a cloned/local directory tree. |
| `localFindFiles` | Find cloned/local files by path metadata. |
| `localFindDeadCode` | Find likely-unreferenced exports in a cloned repository. |
| `lspGetSemantics` | Query definitions, references, callers, callees, types, and symbols in cloned code. |

Use `npx octocode tools --json` for the enabled catalog and
`npx octocode tools <name> --scheme` for an exact schema. Those calls count.
Typical forms:

```bash
npx octocode tools ghSearchCode --queries '{"owner":"OWNER","repo":"REPO","keywords":["TERM"]}'
npx octocode tools ghGetFileContent --queries '{"owner":"OWNER","repo":"REPO","path":"PATH","branch":"SHA","matchString":"SYMBOL"}'
npx octocode tools ghViewRepoStructure --queries '{"owner":"OWNER","repo":"REPO","branch":"SHA","path":"PATH"}'
npx octocode tools ghSearchPullRequests --queries '{"owner":"OWNER","repo":"REPO","prNumber":123}'
```

Prefer targeted remote reads. Use `ghCloneRepo` followed by local search/LSP when
the question needs repeated reads, structural matching, or semantic connections.
An opt-in tool that is disabled is unavailable, not a failed requirement.

## `gh` + RTK arm

RTK is the transport; GitHub CLI supplies the research operations. Every call is
`rtk gh <gh arguments>`. Allowed read-only forms:

```bash
rtk gh search code|repos|prs|issues|commits ...
rtk gh repo view OWNER/REPO ...
rtk gh pr view|diff NUMBER --repo OWNER/REPO ...
rtk gh issue view NUMBER --repo OWNER/REPO ...
rtk gh api 'repos/OWNER/REPO/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'
rtk gh api 'repos/OWNER/REPO/git/trees/SHA?recursive=1'
```

`pr view`, `pr diff`, and `issue view` can be filtered by RTK. `search`, `api`,
and explicit `--json` output are passthrough. Prefer snippet-bearing searches,
raw file media, and minimal JSON fields. RTK adds no repositories or evidence.

## `gh` + Headroom arm

Headroom is already wired into the checked-in wrapper. Do **not** call
`headroom`, `headroom compress`, or a retrieval API. Every GitHub call is:

```bash
./bin/ghc search code|repos|prs|issues|commits ...
./bin/ghc repo view OWNER/REPO ...
./bin/ghc pr view|diff NUMBER --repo OWNER/REPO ...
./bin/ghc issue view NUMBER --repo OWNER/REPO ...
./bin/ghc api 'repos/OWNER/REPO/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'
./bin/ghc api 'repos/OWNER/REPO/git/trees/SHA?recursive=1'
```

The wrapper runs read-only `gh`, compresses once, logs the transform, and emits
exactly what enters context. Prefer tight searches, raw file media, and minimal
JSON fields; compression does not replace query discipline.
