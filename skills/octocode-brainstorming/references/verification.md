Load when a ledger row is not verified, a gatherer or the parent runs the verifier, or a deciding source is a PDF, document, data file, or image.

# Verification and assets

## Run the verifier

`scripts/verify-claims.mjs --ledger <file> --out <file>.verified.jsonl` fetches each cited page, checks every quote part, and exits 0 only when every row is `verified` or `verified-manual`; `verifiedRatio` shows how much holds.

| Status | Action |
|---|---|
| `verified`, `verified-manual` | Evidence; cite the exact verified URL; check its date against the window. |
| `quote-not-found`, `quote-too-short`, `missing-field` | Recopy once as plain prose (no math, tables, symbols), else drop and count it. A manual mark never overrides these. |
| `blocked`, `unsupported-content`, `fetch-failed` | Not shown wrong. `blocked`: rerun once with `--concurrency 1`. Then read via host fetch, file reader, or `octocode-scraping`; if the quote is there, add `"verifiedBy":"manual"` and `"method"` (how read) and rerun → `verified-manual`. |
| `refused` | Private, local, or non-http host, or an asset whose path, URL, or hash does not match its manifest; never evidence. Drop and report. |
| Another page version (abstract vs full text) | Verify separately. |

## When a deciding source is a file

Only the parent downloads a PDF, document, data file, or image; gatherers never do.

1. Run `scripts/fetch-asset.mjs --run-dir <workspace>/.octocode/octocode-brainstorming/tmp/<run> --url <url> ...` without `--approve`: it saves nothing and prints each file's host, type, size, or refusal.
2. Show the plan at a checkpoint; ask the user which to download. Propose only primary sources (publisher, official docs, standards body, government, authors' repository), never mirrors, file-sharing sites, or shorteners.
3. Rerun with `--approve` and only the approved URLs. Files land in `<run>/assets/` with a `manifest.jsonl` (origin URL, size, SHA-256).

A file a host fetch tool saved on its own is not approved: never quote it; offer its URL in the plan.

Workers read assets with the host file reader. A claim from an asset keeps its origin `url` and adds `"asset"` (the path returned by `fetch-asset.mjs`). `verify-claims.mjs` requires that path to stay inside the ledger run's `assets/`, match an entry for the claim URL in `manifest.jsonl`, and retain its recorded SHA-256. A missing, changed, or unbound asset is not evidence. The verifier checks text assets and PDFs when `pdftotext` is installed; otherwise the parent reads the quote and adds `"verifiedBy":"manual"` with a `"method"`.
