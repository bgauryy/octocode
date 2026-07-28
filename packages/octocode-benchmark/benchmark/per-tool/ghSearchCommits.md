# ghSearchCommits

Walk a GitHub repository's commit history for a path or range — when/why an area
changed — **or compare two refs** (`base`+`head`). Needs `owner`+`repo`; scope
with `path`, bound with `since`/`until`. (Replaces `ghHistoryResearch type:"commits"`.)

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghSearchCommits --scheme`) — key ones

| param | type | notes |
|---|---|---|
| owner / repo | string **req** | repo scope |
| path | string | file/dir prefix; trailing / = subtree |
| since / until | string | date/window bounds (relative windows OK, e.g. `30d`) |
| branch | string | ref to walk; default branch otherwise |
| author / committer | string | filter by commit author / committer login |
| base / head | string | **compare mode**: diff `base…head` (ahead/behind + commits) |
| includeDiff | boolean | attach per-commit diffs (costly) |
| itemsPerPage / page | int | history paging |

## Checks

1. **Path history** — `$CLI tools ghSearchCommits --queries '{"owner":"bgauryy","repo":"octocode","path":"packages/octocode-tools-core/src","itemsPerPage":5}' --compact`
   → PASS: dated commits with SHAs.
2. **Windowed** — `... "since":"30d"` → PASS: only recent commits.
3. **With diffs** — `... "includeDiff":true,"itemsPerPage":3` → PASS: per-commit patches (costly).
4. **Author filter** — `... "author":"bgauryy"` → PASS: only that author's commits.
5. **Compare refs** — `... "base":"v2.0.0","head":"v3.0.0"` → PASS: `aheadBy`/`behindBy`/`totalCommits` + the commit list between the two refs.

## Workflows

- **Blame a line's history**: `path` + `includeDiff` → trace when a line changed → escalate the SHA to `ghGetFileContent` at that ref.
- **Release delta**: `base`/`head` two tags (see `ghListReleases`) → the commits that shipped between them, in one call.
