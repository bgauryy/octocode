# ghSearchPullRequests

Search GitHub pull requests, or read one PR's files, diffs, and reviews. List
mode uses `keywordsToSearch` + filters; detail mode needs `owner`+`repo`+`prNumber`
and `content` selectors. Archaeology: `state:"merged"` + `sort:"created"` +
`order:"asc"`. (Replaces `ghHistoryResearch type:"prs"`.)

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghSearchPullRequests --scheme`) — key ones

| param | type | notes |
|---|---|---|
| owner / repo | string | repo scope |
| keywordsToSearch | array<string> | across title/body/comments |
| state | enum(open,closed,merged) | `merged` = is:merged |
| author / reviewed-by / review-requested / label / base / head / created / merged-at | filters | PR search qualifiers |
| checks / review / draft | filters | CI status / review state / draft |
| sort / order | enums | `asc`+`created` = archaeology |
| concise | boolean | flat `#number title` triage |
| prNumber | int | detail mode; needs owner+repo |
| reviewMode | "full" | body+files+patches+comments+reviews+commits |
| content.{body,changedFiles,patches,comments,reviews,commits} | object | pick only what you need |
| content.patches.mode | enum(none,selected,all) | `selected`+files = cheapest diff read |
| filePage / commentPage / commitPage / charOffset / commentBodyOffset | paging | per-surface continuation |
| minify | enum(none,standard) | patches; `none` = exact diff |

## Checks

1. **PR search (triage)** — `$CLI tools ghSearchPullRequests --queries '{"owner":"bgauryy","repo":"octocode","keywordsToSearch":["localSearchCode"],"concise":true,"limit":5}' --compact`
   → PASS: flat `#num title` rows.
2. **PR deep read** — `... '{"owner":"bgauryy","repo":"octocode","prNumber":<n>,"reviewMode":"full"}'`
   → PASS: body + changedFiles + patches + reviews; per-surface `contentPagination`.
3. **Selected patch** — `content.patches.mode:"selected"` + `files:[...]` → PASS: only listed files' diffs.
4. **Archaeology** — `state:"merged","sort":"created","order":"asc"` → PASS: oldest merged PR first.

## Workflows

- **Which PR introduced X**: `keywordsToSearch` + `state:"merged"` → `prNumber` deep read → cite PR#, files, net delta.
- **Verify a fix**: read the PR patches, then confirm against **current** source with `ghGetFileContent`/`localSearchCode` (don't trust the PR description).
- Next: for the commits behind a PR use `ghSearchCommits`; for the issue it closes use `ghSearchIssues`.
