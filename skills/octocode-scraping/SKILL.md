---
name: octocode-scraping
description: "Use when extracting or mapping public web content into a local cited corpus: scrape/crawl a URL or docs site, pull tables/pricing/product fields, diagnose blocked or thin pages, or answer from saved pages. Phrases like scrape this URL, crawl the docs, build a corpus, extract pricing. Prefer keyless fetch; ask before hosted spend. Live clicks/HAR/perf → octocode-chrome-devtools."
---

# Octocode Scraping

Flow: `FRAME → POLICY → ROUTE → FETCH → CORPUS → SEARCH → CITE → RECOVER`.

Chat answers stay in chat; corpora/runs use `<workspace>/.octocode/tmp/scrape/`, durable reports `<workspace>/.octocode/octocode-scraping/`, and approved source/config edits keep their paths. Never use user-level Octocode home for artifacts.

Frame URL/domain, goal, depth, and output before fetching; vague scope → `references/user-inputs.md`. Default to one public URL, `--mode html`, no explicit provider (keyless `cdp`→`direct`), `.octocode/tmp/scrape/{sessionId}`, and compact stdout. Search an existing corpus before refetching. Live interaction belongs to `octocode-chrome-devtools`; process its HAR into the same session.

Ask before auth, hosted spend, crawl expansion, CAPTCHA/MFA, personal-data export, form submits, purchases, sends, deletes, or account changes. Stop after two same-class failures, a hosted `403`, an auth/challenge gate, one failed CDP escalation, sufficient saved evidence, or before expanding a crawl whose summary is not yet useful. Use `references/failure-recovery.md`; cite artifact paths plus URL metadata, never raw dumps.

## Route

- When fetching/crawling/extracting, run `scripts/fetch.mjs --url <u> [--mode html] [--crawl --same-domain --max-pages <n>] [--no-raw]`; when a brief is also needed, run `scripts/fetch-and-brief.mjs --url <u>`.
- Before routing/spend → `scripts/provider-check.mjs [--provider <p>]`; credit status → `scripts/provider-usage.mjs`. Both sanitize secrets.
- Saved session → `scripts/corpus-inspect.mjs --session-dir <d> [--page <n>]`, then `scripts/corpus-find.mjs --session-dir <d> --query <t>`.
- When querying static DOM/assets/paths, run `scripts/dom-find.mjs`, `scripts/resource-list.mjs`, or `scripts/graph-navigate.mjs` with `--session-dir <d>`; live DOM stays in chrome-devtools.
- Local field proof → `scripts/corpus-run.mjs --session-dir <d> --roots cdp,extracts --regex <re>` or `--script <file>`.
- CDP bridge → `scripts/har-ingest.mjs --session-dir <d> --from-cdp-dir <run>`; reverse with `--export-packet`.
- When extraction field names are unclear, run `scripts/schema-helper.mjs --intent "extract pricing and features"`.
- When an old transcript names `scripts/scrapingant-fetch.mjs`, `scripts/scrapingant-check.mjs`, or `scripts/scrapingant-usage.mjs`, treat them as forwarding shims and use the neutral scripts above.

Every runnable script accepts `--help`. Before changing scripts or providers, read `scripts/README.md`; shared modules live in `scripts/lib/`, vendored env resolution in `scripts/octocode-config.mjs`, and JSON contracts in `scripts/schemas/`.

## References

- When scope, policy, or route is unclear, load `references/user-inputs.md`, `references/scraping-policy.md`, or `references/route-selection.md`.
- When choosing a provider, load `references/providers.md`; after hosted approval, load `references/scrapingant.md`; for human setup/vendor extension, read `docs/PROVIDERS.md` or `docs/ADDING_A_VENDOR.md`.
- When searching corpus layout/contracts, load `references/session-corpus.md` and `references/data-contract.md`; for graph/workflows, load `references/website-analysis.md`; for extraction/citations, load `references/extraction-quality.md`.
- When bridging a live browser, load `references/browser-scraping.md`; for blocked/thin/oversized output, load `references/failure-recovery.md`.
