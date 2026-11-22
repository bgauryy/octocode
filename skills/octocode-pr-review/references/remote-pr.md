Load when identifying a remote PR's head and base, retrieving its diff or checking CI at the reviewed revision. Follow the [entry’s authority rules](../SKILL.md).

# Read a remote PR

Call shapes below run over MCP or CLI ([invocation, paging and recovery](octocode-usage.md)); each query also takes `mainGoal` and `reasoning`. When the next call is unclear, read the tool-flow guide in `octocode-research` (MCP resource `octocode://research/tool-flow`).

| Need | `ghGetHistoryItem` with `operation:"pullRequest"`, `owner`, `repo`, `number` |
|---|---|
| `headCommitSha`, `baseCommitSha`, `mergeCommitSha`, size | no sections |
| Changed files, to rank | `sections:["files"]` |
| Patches of chosen files | `sections:["patches"]`, `include:[paths]` |
| A known literal in the diff | add `matchString`, `contextLines:{before:0,after:0}` |
| Intent or an earlier objection | `sections:["body"]`, `["reviews","reviewComments"]`, `["commits"]`, only when they answer a question |

Then read beyond the diff at the same revision:

- **One span:** `ghGetFileContent` with `ref` set to `headCommitSha`, plus `matchString` and `block:true` or `ranges`.
- **Search, AST or LSP:** when `git rev-parse HEAD` matches `headCommitSha`, use the local checkout. Otherwise materialize: `ghStructure` with `operation:"tree"`, `ref` set to `headCommitSha`, `include`, `materialize:true`, and follow `next.continueMaterialize` until `coverage.state` is `complete`. Use the returned `location.path` for search and AST. `lspSearch` refuses fetched code unless the user set `lsp.trustFetchedCode`; without it, say cross-file identity at that head is unverified.
- **Never `ghSearchCode` for PR code:** it searches only the default branch, so it cannot see the head.
- **CI:** with a shell, `gh pr checks <number>`. A result counts only for the head you pinned.

Finish every `sectionPagination` page for each file claimed as reviewed.
