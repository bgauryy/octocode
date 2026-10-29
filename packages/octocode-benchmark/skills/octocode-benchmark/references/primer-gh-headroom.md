# gh + Headroom arm primer

Inject as the `headroom` runner's only primer. Headroom is a transport/compression layer wired
into the checked-in wrapper — it adds no repositories or evidence, only shrinks what a `gh` call
returns. Do **not** call `headroom`, `headroom compress`, or any retrieval API. Every GitHub call
goes through `compare/bin/ghc` (by explicit path):

```bash
compare/bin/ghc search code|repos|prs|issues|commits ...
compare/bin/ghc repo view OWNER/REPO ...
compare/bin/ghc pr view|diff NUMBER --repo OWNER/REPO ...
compare/bin/ghc issue view NUMBER --repo OWNER/REPO ...
compare/bin/ghc api 'repos/OWNER/REPO/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'
compare/bin/ghc api 'repos/OWNER/REPO/git/trees/SHA'          # one level; add ?recursive=1 ONLY when you truly need the whole subtree
```

Read-only only — the wrapper rejects mutating verbs and non-GET `api` with exit 2. It runs `gh`,
compresses once, logs the transform, and emits exactly what enters context.

## What each family is for

- **`search code`** — discovery; a snippet that answers ends the question. **`search prs|issues|commits`** — archaeology.
- **`pr view|diff`, `issue view`** — read one item. **`api .../contents/PATH`** — read one file. **`api .../git/trees/SHA`** — tree shape.

## Leanest path (required — compression is on top of query discipline, not instead of it)

- **Prefer a snippet-bearing `gh search code`** hit that answers outright — avoid any file fetch.
- **File content:** raw media (`-H 'Accept: application/vnd.github.raw'`), never base64 (~1.33× larger before compression). Whole-file raw fetch only when you truly need most of the file.
- **Do NOT dump a whole recursive tree or a whole large file when a targeted path or search answers** — it inflates char cost and is a fairness violation. Use `git/trees/SHA` at a **subpath** (or a few `contents` reads) before `?recursive=1` on the root. Never re-fetch the same file/tree twice.
- **Keep `--json` field lists minimal** — Headroom can compress JSON further but can't recover fields you never needed.

## Know what compresses (choose the path Headroom helps most)

- **Structured JSON** (`api`, `--json`, tree/issue/PR lists) → **SmartCrusher, lossless** — folds repeated keys into one schema header, keeps every value. Ask for structured output where you have the choice.
- **Prose** (issue/PR bodies, diffs) → **neural Kompress, lossy** — great char savings but detail can drop; when you need an exact quote/value, read the precise region and don't rely on the compressed prose for verbatim bytes.
- `ratio=0.000` via `router:noop` is a legitimate passthrough (short/unsupported content), not an error.

Freeze every mutable ref (branch/PR-state/SHA + UTC) before answering; use the frozen ref.
