# ghSearchCommits

Walk a GitHub repository's commit history for a path or range — when/why an area
changed. Needs `owner`+`repo`; scope with `path`, bound with `since`/`until`.
(Replaces `ghHistoryResearch type:"commits"`.)

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghSearchCommits --scheme`) — key ones

| param | type | notes |
|---|---|---|
| owner / repo | string **req** | repo scope |
| path | string | file/dir prefix; trailing / = subtree |
| since / until | string | date/window bounds |
| branch | string | ref to walk; default branch otherwise |
| includeDiff | boolean | attach per-commit diffs (costly) |
| sort / order | enums | author-date / committer-date; asc/desc |
| perPage / page | int | history paging |

## Checks

1. **Path history** — `$CLI tools ghSearchCommits --queries '{"owner":"bgauryy","repo":"octocode","path":"packages/octocode-tools-core/src","perPage":5}' --compact`
   → PASS: dated commits with SHAs.
2. **Windowed** — `... "since":"30d"` → PASS: only recent commits.
3. **With diffs** — `... "includeDiff":true,"perPage":3` → PASS: per-commit patches (costly).

## Workflows

- **Blame a line's history**: `path` + `includeDiff` → trace when a line changed → escalate the SHA to `ghGetFileContent` at that ref.
- **Release delta**: `since`/`until` between two tags (see `ghListReleases`) → the commits that shipped.
