---
name: octocode-scraping
description: "Use when fetching public URLs or crawling a site into a local corpus for repeated queries: docs, pricing tables, link maps, or content extraction. Verify facts in source text. Not for JS-rendered content or live interaction — use octocode-chrome-devtools."
---

# Octocode Scraping

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-chrome-devtools`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Flow: `FRAME → POLICY → ROUTE → FETCH → CORPUS → READ → CITE → RECOVER`.

Corpora/runs: `<output>/tmp/scrape/`; reports: `<output>/octocode-scraping/`. Chat answers stay in chat; approved source/config edits keep their paths.

Frame URL/domain, goal, depth, and output before fetching; vague scope → `references/user-inputs.md`. Default to one public URL, `--mode html`, no explicit provider (bounded direct HTTP), `.octocode/tmp/scrape/{sessionId}`, and compact stdout. Search an existing corpus before refetching. A `next.route: octocode-chrome-devtools` result means the saved direct evidence is blocked or looks like a thin application shell: render that page once, then bridge the retained browser artifact into the same session. Live interaction belongs to `octocode-chrome-devtools`.

**Context gate:** Use URL metadata, titles, snippets, and literal search to select a small source span. For classification, delegate to `octocode-clasify` and its decision gate; otherwise read directly. Read only enough source to answer the question.

For repo, package, or code claims, use `octocode-research`. Keep URL fetching and corpus extraction in this skill.

Ask before auth, hosted spend, crawl expansion, CAPTCHA/MFA, personal-data export, form submits, purchases, sends, deletes, or account changes. Stop after two same-class failures, a hosted `403`, an auth/challenge gate, one failed CDP escalation, or enough saved evidence. Stop before expanding a crawl whose summary is not yet useful. Use `references/failure-recovery.md`; cite artifact paths plus URL metadata, never raw dumps.

## Route

- When fetching/crawling/extracting, run `scripts/fetch.mjs --url <u> [--mode html] [--crawl --same-domain --max-pages <n>] [--no-raw]`; when a brief is also needed, run `scripts/fetch-and-brief.mjs --url <u>`. Direct HTTP identifies itself, caps bytes while streaming, honors a short `Retry-After` once, and checks robots rules for crawls. **CDP escalation path:** follow `next` or escalate when target text remains absent after checking wording and extraction quality. Load `octocode-chrome-devtools`, run `open-browser + page-snapshot + dom-operations-check` on the URL, then bridge results back with `scripts/har-ingest.mjs --session-dir <existing-session>`. Do not start a new scrape session.
- Before routing/spend → `scripts/provider-check.mjs [--provider <p>]`; credit status → `scripts/provider-usage.mjs`. Both sanitize secrets.
- For a classification request admitted by `octocode-clasify`, run the Octocode CLI recipe in `references/clasify-screen.md` against retained corpus artifacts.
- When navigating a saved session, run `scripts/corpus-inspect.mjs --session-dir <d> [--page <n>]`; for bounded text search, run `scripts/corpus-find.mjs --session-dir <d> --query <t>`. Retrieve the smallest sufficient source span.
- When querying static DOM/assets/paths, run `scripts/dom-find.mjs`, `scripts/resource-list.mjs`, or `scripts/graph-navigate.mjs` with `--session-dir <d>`; live DOM stays in chrome-devtools.
- Local field proof → `scripts/corpus-run.mjs --session-dir <d> --roots cdp,extracts --regex <re>` or `--script <file>`.
- CDP bridge → `scripts/har-ingest.mjs --session-dir <d> --from-cdp-dir <run>`; reverse with `--export-packet`.
- When field names are unclear, run `scripts/schema-helper.mjs --intent "extract pricing and features"`.
- When an old transcript names `scripts/scrapingant-fetch.mjs`, `scripts/scrapingant-check.mjs`, or `scripts/scrapingant-usage.mjs`, treat them as forwarding shims and use the neutral scripts above.

Every runnable script accepts `--help`. Before changing scripts or providers, read `scripts/README.md`; shared modules live in `scripts/lib/`, vendored env resolution in `scripts/octocode-config.mjs`, and JSON contracts in `scripts/schemas/`.

After corpus-search changes, run `node --test scripts/tests/corpus-find.test.mjs`; after fetch/session changes, run `node --test scripts/tests/fetch-session.test.mjs`; after CDP client changes, run `node --test scripts/tests/cdp-client.test.mjs`. These finite local regressions need no browser or hosted provider; they do not replace a live browser check for CDP integration changes.

## References

- For admitted classification requests, `references/clasify-screen.md` maps saved pages to an executable `octocode clasify` call.
- When scope, policy, or route is unclear, load `references/user-inputs.md`, `references/scraping-policy.md`, or `references/route-selection.md`.
- When choosing a provider, load `references/providers.md`; after hosted approval, load `references/scrapingant.md`; for human setup/vendor extension, read `docs/PROVIDERS.md` or `docs/ADDING_A_VENDOR.md`.
- When searching corpus layout/contracts, load `references/session-corpus.md` and `references/data-contract.md`; for graph/workflows, load `references/website-analysis.md`; for extraction/citations, load `references/extraction-quality.md`.
- When bridging a live browser, load `references/browser-scraping.md`; for blocked/thin/oversized output, load `references/failure-recovery.md`.
