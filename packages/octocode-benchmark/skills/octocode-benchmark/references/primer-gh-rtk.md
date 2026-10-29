# gh + RTK arm primer

Inject as the `rtk` runner's only primer. RTK is the transport/filter; GitHub CLI supplies the
research operations. Every call is `rtk gh <gh arguments>`. RTK adds no repositories or
evidence — it only filters/compresses what a `gh` call returns. Allowed read-only forms:

```bash
rtk gh search code|repos|prs|issues|commits ...
rtk gh repo view OWNER/REPO ...
rtk gh pr view|diff NUMBER --repo OWNER/REPO ...
rtk gh issue view NUMBER --repo OWNER/REPO ...
rtk gh api 'repos/OWNER/REPO/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'
rtk gh api 'repos/OWNER/REPO/git/trees/SHA'            # one level; add ?recursive=1 ONLY when you truly need the whole subtree
```

Read-only only — no mutation verb and no non-GET `api`.

## What each family is for

- **`search code`** — discovery: find the repo/path/snippet. A snippet that already answers the question ends it.
- **`search prs|issues|commits`** — archaeology: how/why a change landed, when an area changed, related discussion.
- **`pr view|diff`, `issue view`** — read one item's body/diff/review.
- **`api .../contents/PATH`** — read one file. **`api .../git/trees/SHA`** — inspect tree shape.

## Leanest path (required)

- **Prefer a snippet-bearing `gh search code`** hit that answers outright — avoid any file fetch.
- **File content:** raw media (`-H 'Accept: application/vnd.github.raw'`), never base64 `--jq .content` (~1.33× larger). `gh` has no server-side region read, so a whole-file raw fetch is legitimate **only** when you genuinely need most of the file.
- **Do NOT dump a whole recursive tree or a whole large file when a targeted path or search answers** — that inflates your char cost and is a fairness violation. Reach for `git/trees/SHA` at a **subpath** (or a couple of `contents` reads) before `?recursive=1` on the repo root; use `?recursive=1` only when the question truly needs the entire subtree. Never re-fetch the same file/tree twice.
- **Keep `--json` field lists minimal.**

## Where RTK helps (choose the filtered path)

- `pr view`, `pr diff`, `issue view` **without `--json`** get RTK's compaction (measured ~50% smaller on `pr view`) — prefer these.
- `search`, `api`, and any explicit `--json` are **passthrough** — RTK adds nothing; keep those queries tight on your own.

Freeze every mutable ref (branch/PR-state/SHA + UTC) before answering; use the frozen ref.
