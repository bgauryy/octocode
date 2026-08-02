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
| itemsPerPage / page / limit | int | history paging / result cap |

## Checks

1. **Path history** — `$CLI tools ghSearchCommits --queries '{"owner":"bgauryy","repo":"octocode","path":"packages/octocode-tools-core/src","itemsPerPage":5}' --compact`
   → PASS: dated commits with SHAs.
2. **Windowed** — `... "since":"30d"` → PASS: only recent commits.
3. **With diffs** — `... "includeDiff":true,"itemsPerPage":3` → PASS: per-commit patches (costly).
4. **Author filter** — `... "author":"bgauryy"` → PASS: only that author's commits.
5. **Compare refs** — `... "base":"<real-tag-or-sha>","head":"<real-tag-or-sha>"` (verify the refs exist first, e.g. via `ghListReleases`) → PASS: `aheadBy`/`behindBy`/`totalCommits` + the commit list between the two refs.
6. **includeDiff + directory path** — `... '{"owner":"bgauryy","repo":"octocode","path":"packages/octocode-tools-core/src","includeDiff":true,"itemsPerPage":2}'` → PASS: per-commit `files[]` (changed files under that directory, with patches) plus a warning that the path was treated as a directory filter — NOT bare commits with no diff and no explanation. Regression guard: a directory path (no trailing `/`) used to be classified as a file, silently dropping `includeDiff` entirely.
7. **includeDiff + exact file path** — same query with a real file path (e.g. `.../deadCodeScan.ts`) → PASS: `patch`/`additions`/`deletions` attached directly to each commit.
8. **Honest bad-ref failure** — compare with a fabricated `base` ref and real `head` → PASS: non-success diagnostic says the ref cannot be resolved; it must not fabricate `aheadBy`/`behindBy` or an empty-history proof.

## Workflows

- **Blame a line's history**: `path` + `includeDiff` → trace when a line changed → escalate the SHA to `ghGetFileContent` at that ref.
- **Release delta**: `base`/`head` two tags (see `ghListReleases`) → the commits that shipped between them, in one call.
