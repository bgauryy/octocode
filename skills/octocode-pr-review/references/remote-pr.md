Load when identifying a remote PR's head and base, retrieving its diff or checking CI at the reviewed revision. Follow the [entry’s authority rules](../SKILL.md).

# Read a remote PR

Call shapes below run over MCP or CLI (CLI: add `--all-pages` to follow every page). When the next call is unclear, read the tool-flow guide in `octocode-research` (MCP resource `octocode://research/tool-flow`).

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
- **CI:** with an authenticated `gh`, read results by the pinned SHA, not the PR number: `gh pr checks` shows the current head and names no commit.

  ```bash
  gh api --paginate "repos/<owner>/<repo>/commits/<headCommitSha>/check-runs?per_page=100" \
    --jq '.check_runs[] | [.head_sha, .status, (.conclusion // "-"), .name, .html_url] | @tsv'
  gh api --paginate "repos/<owner>/<repo>/commits/<headCommitSha>/status?per_page=100" \
    --jq '"statuses: \(.total_count)", (.statuses[] | [.state, .context, .target_url] | @tsv)'
  gh pr view <number> -R <owner>/<repo> --json headRefOid --jq .headRefOid
  ```

  | Row at that SHA | Record in `## Checks` as |
  |---|---|
  | Check run `completed` with `conclusion` `success` or `neutral` | `passed` |
  | Check run `completed` with `conclusion` `skipped` | `skipped` |
  | Check run with `conclusion` `failure` or `timed_out` | `failed` |
  | Any other check run (`cancelled`, `action_required`, `stale`, or `status` not `completed`) | `unavailable`: it did not finish at that SHA |
  | Status `state` `success` / `failure` or `error` / `pending` | `passed` / `failed` / `unavailable` |
  | No check runs and `statuses: 0` | `unavailable`: no CI ran at that SHA |

  A `headRefOid` other than the pinned SHA means the PR moved: these results still cover the reviewed revision; state that the newer head was not reviewed.

Finish every `sectionPagination` page for each file claimed as reviewed.
