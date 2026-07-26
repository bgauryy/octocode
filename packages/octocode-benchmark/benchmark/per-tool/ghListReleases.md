# ghListReleases

List a GitHub repository's releases (tagName, publishedAt, prerelease flag) and
surface the latest stable release. Needs `owner`+`repo`. (Replaces
`ghHistoryResearch type:"releases"`.)

> **Opt-in tool.** Disabled by default to keep the toolset lean — enable with
> `ENABLE_RELEASES=1`. Niche surface; most release questions are better served by
> `ghSearchCommits` (what shipped) or `ghGetFileContent` (a file at a tag).

```bash
CLI="node packages/octocode/out/octocode.js"
```

## Params (`tools ghListReleases --scheme`) — key ones

| param | type | notes |
|---|---|---|
| owner / repo | string **req** | repo scope |
| limit / page / itemsPerPage | int | release-list paging |

## Checks

1. **Releases** — `ENABLE_RELEASES=1 $CLI tools ghListReleases --queries '{"owner":"microsoft","repo":"TypeScript","limit":5}' --compact`
   → PASS: tagName/publishedAt rows + latest stable surfaced.

## Workflows

- **What's the latest stable?** → `ghListReleases` → feed the tag into `ghViewRepoStructure`/`ghGetFileContent` at that ref.
- **What shipped between releases?** → take two tags → `ghSearchCommits` with `since`/`until`.
