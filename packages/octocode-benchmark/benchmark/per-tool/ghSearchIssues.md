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
| author / assignee / commenter / mentions / label / created / closed | filters | issue search qualifiers |
| sort / order | enums | `asc`+`created` = archaeology |
| concise | boolean | flat `#number title` triage |
| issueNumber | int | detail mode; needs owner+repo |
| content.{body,comments} | object | pick only what you need |
| content.comments.{discussion,includeBots} | object | discussion comments; bots off by default |
| commentPage / charOffset / commentBodyOffset | paging | per-surface continuation |
| minify | enum(none,standard) | body/comment compaction |

## Checks

1. **Issue search (triage)** — `$CLI tools ghSearchIssues --queries '{"owner":"bgauryy","repo":"octocode","keywordsToSearch":["bug"],"state":"open","concise":true,"limit":5}' --compact`
   → PASS: flat `#num title` rows.
2. **Issue detail** — `... '{"owner":"bgauryy","repo":"octocode","issueNumber":<n>,"content":{"body":true,"comments":{"discussion":true}}}'` → PASS: body + comments.
3. **Label triage** — `... '{"owner":"facebook","repo":"react","label":"bug","state":"open","limit":5}'` → PASS: only labeled issues.

## Workflows

- **RCA on a bug**: issue detail → find the linked fix PR with `ghSearchPullRequests` → verify the fix against **current** source.
- **Triage backlog**: `label` + `state:"open"` + `sort:"reactions"` → most-wanted issues first.
