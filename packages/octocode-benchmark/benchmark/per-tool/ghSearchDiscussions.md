# ghSearchDiscussions

Search a GitHub repository's Discussions (Q&A, RFCs, announcements) — the
community/maintainer surface that PRs, issues, and commits don't cover. Needs
`owner`+`repo`; `keywordsToSearch` filters title/body (omit to list newest).

> **Opt-in tool.** Disabled by default — enable with `ENABLE_DISCUSSIONS=1`.
> Discussions are **GraphQL-only** (no REST list endpoint), so this is the one
> tool that talks to GitHub's GraphQL API. Niche surface; most questions are
> better served by `ghSearchIssues` (bugs) or `ghSearchPullRequests` (changes).

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghSearchDiscussions --scheme`) — key ones

| param | type | notes |
|---|---|---|
| owner / repo | string **req** | repo scope |
| keywordsToSearch | string[] | terms matched in title/body; omit to list newest |
| itemsPerPage | int 1–100 | discussions per page |
| after | string | cursor from a prior response's `pagination.nextCursor` |

## Checks

1. **List newest** — `ENABLE_DISCUSSIONS=1 $CLI tools ghSearchDiscussions --queries '{"owner":"vercel","repo":"next.js","itemsPerPage":3}' --compact`
   → PASS: `totalCount` + rows (number/title/url/author/category/comments); `pagination.nextCursor` when more.
2. **Keyword search** — `... "keywordsToSearch":["turbopack"]` → PASS: `totalCount` drops to the matching subset.
3. **Cursor paging** — feed `pagination.nextCursor` back as `after` → PASS: the next page continues (the `next.nextPage` hint is ready-to-run).
4. **Answered flag** — Q&A discussions with an accepted answer carry `answered: true`.

## Workflows

- **Triage a question**: search discussions for the symptom → open the discussion URL → if it's an unresolved bug, cross-check `ghSearchIssues`.
- **Find the RFC behind a change**: `keywordsToSearch` the feature name → the design discussion → then `ghSearchPullRequests` for the implementation.
