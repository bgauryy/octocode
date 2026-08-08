# gh arm primer

Inject as the `gh` runner's only primer. This is the **uncompressed baseline** — plain GitHub CLI,
no transport wrapper, no compression. Every research call is `gh <args>` (run through the arm's
instrumentation wrapper). This is the raw context cost the `rtk` and `headroom` matchups compress.
Allowed read-only forms:

```bash
gh search code|repos|prs|issues|commits ...
gh repo view OWNER/REPO ...
gh pr view|diff NUMBER --repo OWNER/REPO ...
gh issue view NUMBER --repo OWNER/REPO ...
gh api 'repos/OWNER/REPO/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'
gh api 'repos/OWNER/REPO/git/trees/SHA'          # one level; add ?recursive=1 ONLY when you truly need the whole subtree
```

Read-only only — no mutation verb (`create`, `edit`, `merge`, `close`, `comment`) and no non-GET `api`.

## What each family is for

- **`search code`** — discovery: find the repo/path/snippet; a snippet that answers ends the question.
- **`search prs|issues|commits`** — archaeology: how/why a change landed, when an area changed.
- **`pr view|diff`, `issue view`** — read one item's body/diff/review.
- **`api .../contents/PATH`** — read one file. **`api .../git/trees/SHA`** — inspect tree shape.

## Leanest path (required)

- **Prefer a snippet-bearing `gh search code`** hit that already answers — avoid a full-file fetch.
- **File content:** raw media (`-H 'Accept: application/vnd.github.raw'`), never base64 `--jq .content` (~1.33× larger). `gh` has no server-side region read, so a whole-file raw fetch is legitimate **only** when you genuinely need most of the file.
- **Do NOT dump a whole recursive tree or a whole large file when a targeted path or search answers** — it inflates your char cost and is a fairness violation. Use `git/trees/SHA` at a **subpath** (or a few `contents` reads) before `?recursive=1` on the repo root, and only when the question truly needs the whole subtree. Never re-fetch the same file/tree twice.
- **Keep `--json` field lists minimal.**

Freeze every mutable ref (branch/PR-state/SHA + UTC) before answering; use the frozen ref.
