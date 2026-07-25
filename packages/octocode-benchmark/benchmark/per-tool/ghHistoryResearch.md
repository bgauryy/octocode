# ghHistoryResearch

Search and read GitHub PRs, commit history, issues, and releases.
`type:"prs"` searches PRs (add `prNumber` for detail); `type:"commits"` walks
`owner/repo/path` history; `type:"issues"` searches/reads issues; `type:"releases"`
lists releases. Archaeology: `state:"merged"` + `sort:"created"` + `order:"asc"`.

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghHistoryResearch --scheme`) — key ones

| param | type | notes |
|---|---|---|
| type | enum(prs,commits,releases,issues) | research mode |
| owner / repo | string | required for commits mode |
| keywordsToSearch | array<string> | across title/body/comments |
| state | enum(open,closed,merged) | `merged` = is:merged |
| author / reviewed-by / label / base / created / merged-at | filters | PR/issue search qualifiers |
| sort / order | enums | `asc`+`created` = archaeology |
| concise | boolean | flat `#number title` triage |
| prNumber / issueNumber | int | detail mode |
| reviewMode | "full" | body+files+patches+comments+reviews+commits |
| content.{body,changedFiles,patches,comments,reviews,commits} | object | pick only what you need |
| content.patches.mode | enum(none,selected,all) | `selected`+files = cheapest diff read |
| path / since / until / includeDiff / perPage | commits mode | history + optional per-commit diffs |
| filePage / commentPage / commitPage / charOffset / commentBodyOffset | paging | per-surface continuation |
| minify | enum(none,standard) | patches; `none` = exact diff |

## Checks

1. **PR search (triage)** — `$CLI tools ghHistoryResearch --queries '{"type":"prs","owner":"bgauryy","repo":"octocode","keywordsToSearch":["localSearchCode"],"concise":true,"limit":5}' --compact`
   → PASS: flat `#num title` rows.
2. **PR deep read** — `... '{"type":"prs","owner":"bgauryy","repo":"octocode","prNumber":<n>,"reviewMode":"full"}'`
   → PASS: body + changedFiles + patches + reviews; per-surface `contentPagination`.
3. **Selected patch** — `content.patches.mode:"selected"` + `files:[...]` → PASS: only listed files' diffs.
4. **Commit history** — `... '{"type":"commits","owner":"bgauryy","repo":"octocode","path":"packages/octocode-tools-core/src","perPage":5}'` → PASS: dated commits with SHAs.
5. **Issue detail** — `... '{"type":"issues","owner":"bgauryy","repo":"octocode","issueNumber":<n>,"content":{"body":true,"comments":{"discussion":true}}}'` → PASS: body + comments.
6. **Releases** — `... '{"type":"releases","owner":"microsoft","repo":"TypeScript","perPage":5}'` → PASS: tagName/publishedAt + latest stable surfaced.
7. **Archaeology** — `state:"merged","sort":"created","order":"asc"` → PASS: oldest merged PR first.

## Workflows

- **Which PR introduced X**: `keywordsToSearch` + `state:"merged"` → `prNumber` deep read → cite PR#, files, net delta.
- **RCA on a bug**: issue detail → linked fix PR patches → verify against **current** source with `ghGetFileContent`/`localSearchCode` (don't trust the PR description).
- **Blame a line's history**: `type:"commits"` with `path` + `includeDiff` → trace when a line changed.
