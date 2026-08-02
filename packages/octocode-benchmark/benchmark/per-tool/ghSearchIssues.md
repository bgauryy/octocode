# ghSearchIssues

Search GitHub issues, or read one issue's body and comments. List mode uses
`keywordsToSearch` + filters; detail mode needs `owner`+`repo`+`issueNumber` and
`content` selectors. (Replaces `ghHistoryResearch type:"issues"`.)

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghSearchIssues --scheme`) — key ones

| param | type | notes |
|---|---|---|
| owner / repo | string | repo scope |
| keywordsToSearch | array<string> | across title/body/comments |
| state | enum(open,closed) | issue state |
| author / assignee / commenter / mentions / label / created / updated / closed | filters | issue search qualifiers |
| comments / reactions / archived | filters | activity and archive qualifiers |
| match | array<enum(title,body,comments)> | text fields searched |
| sort / order | enums | `asc`+`created` = archaeology |
| concise | boolean | flat `#number title` triage |
| issueNumber | int | detail mode; needs owner+repo |
| content.{body,comments} | object | pick only what you need |
| matchString | string | anchor/filter body or comment slices where supported |
| content.comments.{discussion,includeBots} | object | discussion comments; bots off by default |
| commentPage / itemsPerPage / charOffset / charLength / commentBodyOffset | paging | per-surface continuation |
| minify | enum(none,standard) | body/comment compaction |

## Checks

1. **Issue search (triage)** — `$CLI tools ghSearchIssues --queries '{"owner":"microsoft","repo":"vscode","keywordsToSearch":["crash"],"state":"open","concise":true,"limit":5}' --compact`
   → PASS: flat `#num title` rows.
2. **Issue detail** — `... '{"owner":"bgauryy","repo":"octocode","issueNumber":<n>,"content":{"body":true,"comments":{"discussion":true}}}'` → PASS: body + comments.
3. **Label triage** — `... '{"owner":"microsoft","repo":"vscode","label":"bug","state":"open","limit":5}'` → PASS: only labeled issues.
4. **Comment pagination / continuation** — read a noisy issue with `content.comments.discussion:true`, small page/char window, then follow `commentPage` / `charOffset` / `commentBodyOffset` → PASS: next page preserves owner/repo/issue/content filters.
5. **Honest empty / missing issue** — impossible label combo or nonexistent `issueNumber` → PASS: empty/404 diagnostic with a repair hint; never treats an empty search as proof that a bug does not exist.

## Workflows

- **RCA on a bug**: issue detail → find the linked fix PR with `ghSearchPullRequests` → verify the fix against **current** source.
- **Triage backlog**: `label` + `state:"open"` + `sort:"reactions"` → most-wanted issues first.
