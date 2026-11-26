Load when deciding whether a source may be cited, a ledger row is not verified, a gatherer or the parent runs the verifier, or a deciding source is a PDF, document, data file, or image.

# Verification and assets

## Validated sources

Cite only a validated source. A source is validated for a claim when every check passes; one that fails stays a lead, and a claim that depends on it is a named gap.

| Check | Passes when |
|---|---|
| Opened | You or a worker read the page or file itself. A search snippet, an AI search answer or its citation list is a lead only. |
| Quote verified | Its ledger row is `verified`, or `verified-manual` after a read the script could not do. A row with `via` was read from an Exa or Tavily copy of the same URL, often cached: check its date like any other. |
| Quote supports the claim | The quote says what the claim says, with nothing added. The verifier checks that the quote is on the page, not what it means. |
| Exact page | The cited URL is the page read: abstract and full text, or two versions of a page, are separate sources. For versioned docs, cite the version the user runs (`/docs/16/`, not `/docs/current/`, which moves); for GitHub, a [commit permalink](research-github.md#github), not `/blob/main/`. |
| Date | Inside the question's window, read from the page, not from the search row: run the verifier with `--since`/`--until` and read each claim's `dateInWindow` (`yes`, `no`, `unknown`) beside its `published` and `modified`. `unknown` means no usable date: check it by hand or label the date unverified. The verifier ignores the server's Last-Modified header (a deploy time) and falls back to a dated sentence in the text (`textDate`, such as "Last tested on …"); a timestamp that two or more pages of one site share in the run is marked `publishedFrom: "site-build"` and ignored. One page cited alone cannot show that its meta date is a build stamp, so read the date printed on the page before relying on it. `dateWindow` totals `yes`, `no` and `unknown`. |
| Who is behind it | Owner, funding, and whether it sells what it praises, checked on other pages. Authors measuring their own method also have a stake. A stake needs independent support. |
| Primary | The original study, spec, release, dataset or incident report. Otherwise label it `secondary` and lower confidence. Trace a finding a summary attributes to a study back to that study and cite what the study found: a summary can claim more than the study showed ([example](method-evidence-challenge.md#premortem)). |
| Independent | Not a syndicated copy, mirror or rewrite of a source already counted. The verifier marks rows citing one page under different URLs with `sameSourceAs` and gives each claim `distinctSources`; a rewrite on another site stays your judgment. |

## Run the verifier

`scripts/verify-claims.mjs --ledger <file> --out <file>.verified.jsonl` fetches cited pages and exits 0 only when every row is `verified` or `verified-manual`. Inspect failed rows; the success ratio is not an evidence-quality score.

When the direct read is `blocked`, `fetch-failed` or `unsupported-content`, the verifier reads one provider copy of the same URL: Exa `/contents` (about $0.001 a page), then Tavily `/extract` (1 credit per 5 pages), whichever has a key. `--via exa|tavily` picks one. A found quote reports `verified` with `via` (provider, `cached` or `crawled`, cost), and `providerCopies` totals the spend per provider (`reusedPages` counts rows that still rest on an earlier copy). `read-page.mjs` and the verifier each pay for their own copy of a walled page. A quote missing from the copy keeps the original status: the page itself was not read, so it is unread, not refuted. The copy is on by default because a source the script cannot read otherwise stays unvalidated; `--via none` turns it off when spend is not authorized.

Several ledgers (one per worker) verify in one run with repeated `--ledger`: the summary counts each and the total. `--out` takes one ledger, so write each verified copy in its own run. On a rerun add `--reuse <file>.verified.jsonl` so only new or changed rows are fetched. For a run over several ledgers, concatenate their verified copies into one file (`cat <run>/ledgers/*.verified.jsonl > <run>/all.verified.jsonl`) and pass that to `--reuse`: every unchanged row is reused, so nothing is fetched again.

| Status | Action |
|---|---|
| `verified`, `verified-manual` | Evidence once the other [validated-source checks](#validated-sources) pass; cite the exact verified URL. |
| `quote-not-found`, `quote-too-short`, `missing-field` | Correct the quote or missing field from the source; if unsupported, exclude the claim and report the gap. A manual mark never overrides these. |
| `blocked`, `unsupported-content`, `fetch-failed` | Not shown wrong, and the provider copy did not settle it (or no key). Change access method: serial fetch (`--concurrency 1`), host fetch, file reader or `octocode-scraping`. If read and matched, add `"verifiedBy":"manual"` and `"method"`, then rerun → `verified-manual`. Do not repeat an unchanged failing request. |
| `refused` | Private, local, or non-http host, or an asset whose path, URL, or hash does not match its manifest; never evidence. Drop and report. |

## When a deciding source is a file

The orchestrator owns downloads and provenance; gatherers return source URLs. Reuse existing authorization for in-scope public-source research. Ask only when the download introduces unresolved scope, spend or authority.

1. Run `scripts/fetch-asset.mjs --run-dir <run> --url <url> ...` without `--approve`: it saves nothing and prints each file's host, type, size, or refusal.
2. Inspect the plan: primary source (publisher, official docs, standards body, government or authors' repository), expected type and size, no refusal. Resolve missing authorization before continuing.
3. Rerun with `--approve` for the authorized URLs only. Files land in `<run>/assets/` with a `manifest.jsonl` (origin URL, size, SHA-256).

A host-saved file may be read when authorized, but its path alone proves no provenance. For a ledger `asset`, use the manifest-bound copy; otherwise cite the opened origin page and record the actual reading method.

| Asset check | Required condition |
|---|---|
| Reader | Workers use the host file reader. |
| Claim identity | Keep origin `url`; add `"asset"` with the path returned by `fetch-asset.mjs`. |
| Binding | Path stays inside the ledger run's `assets/`, matches the claim URL in `manifest.jsonl` and retains its SHA-256. Missing, changed or unbound assets are not evidence. |
| Verification | `verify-claims.mjs` checks text and, with `pdftotext`, PDFs. Otherwise the parent reads the quote and adds `"verifiedBy":"manual"` with `"method"`. |
