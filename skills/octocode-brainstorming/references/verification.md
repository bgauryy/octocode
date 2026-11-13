Load when a ledger row is not verified, a gatherer or the parent runs the verifier, or a deciding source is a PDF, document, data file, or image.

# Verification and assets

## Run the verifier

Use the script when automated quote checking is useful; ordinary sourced answers need no ledger. `scripts/verify-claims.mjs --ledger <file> --out <file>.verified.jsonl` fetches cited pages and exits 0 only when every row is `verified` or `verified-manual`. Inspect failed rows; the success ratio is not an evidence-quality score.

| Status | Action |
|---|---|
| `verified`, `verified-manual` | Evidence; cite the exact verified URL; check its date against the window. |
| `quote-not-found`, `quote-too-short`, `missing-field` | Correct the quote or missing field from the source; if unsupported, exclude the claim and report the gap. A manual mark never overrides these. |
| `blocked`, `unsupported-content`, `fetch-failed` | Not shown wrong. Change access method: serial fetch (`--concurrency 1`), host fetch, file reader or `octocode-scraping`. If read and matched, add `"verifiedBy":"manual"` and `"method"`, then rerun → `verified-manual`. Do not repeat an unchanged failing request. |
| `refused` | Private, local, or non-http host, or an asset whose path, URL, or hash does not match its manifest; never evidence. Drop and report. |
| Another page version (abstract vs full text) | Verify separately. |

## When a deciding source is a file

The orchestrator owns downloads and provenance; gatherers return source URLs. Reuse existing authorization for in-scope public-source research. Ask only when the download introduces unresolved scope, spend or authority.

1. Run `scripts/fetch-asset.mjs --run-dir <workspace>/.octocode/octocode-brainstorming/tmp/<run> --url <url> ...` without `--approve`: it saves nothing and prints each file's host, type, size, or refusal.
2. Inspect the plan: primary source (publisher, official docs, standards body, government or authors' repository), expected type and size, no refusal. Resolve missing authorization before continuing.
3. Rerun with `--approve` for the authorized URLs only. Files land in `<run>/assets/` with a `manifest.jsonl` (origin URL, size, SHA-256).

A host-saved file may be read when authorized, but its path alone proves no provenance. For a ledger `asset`, use the manifest-bound copy; otherwise cite the opened origin page and record the actual reading method.

| Asset check | Required condition |
|---|---|
| Reader | Workers use the host file reader. |
| Claim identity | Keep origin `url`; add `"asset"` with the path returned by `fetch-asset.mjs`. |
| Binding | Path stays inside the ledger run's `assets/`, matches the claim URL in `manifest.jsonl` and retains its SHA-256. Missing, changed or unbound assets are not evidence. |
| Verification | `verify-claims.mjs` checks text and, with `pdftotext`, PDFs. Otherwise the parent reads the quote and adds `"verifiedBy":"manual"` with `"method"`. |
