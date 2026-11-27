Load when reading a script's stdout fields, an error code or exit status, or a truncation flag and the capture limits behind it.

# Script output and capture limits

## Stdout contract

- Capture commands return JSON metadata; query commands can return evidence snippets or source pages. `fetch.mjs` fields: `ok`, `sessionId`, `sessionDir`, `route`, `status`, `contentType`, `pages` (session total), `newPages` (this run), `antCreditsKnownTotal`, `creditCap` (with `--max-credits`: `spentThisRun`, `largestRequest`, `costUnknownAt`, `stopped`, `reason`, `unfetchedUrls`; else `null`), `providerDetail`, `providerRequest` (hosted endpoint and sent options, never the key or page URLs; `null` for direct and cdp), `warnings`, `next` (a `route` handoff such as `octocode-chrome-devtools`, or `null`), `robots`, `agentIndex`, `analysis`, `searchFirst`, `rawAudit`.
- Errors print `{"ok":false,"error":{"code","message"}}` on stdout: exit 2 for invalid input (`invalidArguments`, `sessionExists`, `refusedHost` for a private start URL); exit 1 for a runtime failure such as `sessionNotFound` (`--session-dir` is not a scrape session). Exception: `corpus-inspect`, `dom-find`, `graph-navigate`, `resource-list` print usage on stderr for a missing `--session-dir`; `corpus-inspect --format coverage-md` prints Markdown ([output](../output.md#saved-report)).
- An `--append` run that fetches no new page warns and exits 1.
- A refused crawled link, sitemap entry, or `cdp` request appears in `warnings` as `refused <url>: <reason>`; a refused redirect is that page's `error` in `sources.jsonl`.
- `har-ingest.mjs` prints `ok`, `flow` (`cdp→scrape` | `scrape→cdp-packet`), `sessionDir`, `thinHints`, `cdpFiles`, `extracts`, `next`; never raw HAR or HTML.
- `corpus-run.mjs` prints `ok`, `flow` (`local-iterate`), `sessionDir`/`artifactDir`, `matchCount`, `matches[]` (`file`/`abs`/`line`/`column`/`match`/`snippet`), `next[]` (`file`/`line`/`match`), optional `script.result`.
- Other scripts: `corpus-find.mjs` fields are in [bounded search](corpus.md#bounded-search); `dom-find.mjs` and `resource-list.mjs` fields in [site graph](site-graph.md#site-graph).
- `corpus-run.mjs` default roots: `text,extracts,cdp,snippets,reports,indexes,graph,sources`; add `raw` to audit. `--regex` is JavaScript: `--regex 'price' --flags i`, not `(?i)`.

## Capture limits

- Response limits come from `--max-raw-bytes` and `--max-text-bytes`; inspect `fetch.mjs --help` and returned warnings rather than copying defaults into requests. Bodies are saved to `raw/` unless `--no-raw`, which drops the raw audit. If `networkTruncated`, `rawTruncated` or `textTruncated` is set, raise the relevant limit or report unread scope.
- `--chunk-bytes` and `--max-text-bytes` size clean-text parts; nothing is discarded.
- [source-query.mjs](../scripts/source-query.mjs) pages one file; other scripts' continuations call it. Its default `--encoding auto` returns a UTF-8 file as text pages that end on character boundaries, so the pages' `content` in order is the file exactly; other bytes come back as base64 (`encoding` says which). Text pages of any file: `node <skill-folder>/scripts/source-query.mjs --file <path> --encoding utf8` (`notUtf8`, exit 2, on invalid UTF-8).
