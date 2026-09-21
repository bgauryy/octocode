---
name: octocode-scraping
description: "Use when scraping public URLs/docs into a cited corpus, extracting tables/pricing, or diagnosing blocked/thin pages. Not for live interaction → octocode-chrome-devtools."
---

# Octocode Scraping

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-chrome-devtools`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Flow: `FRAME → POLICY → ROUTE → FETCH → CORPUS → SCREEN → CITE → RECOVER`.

Corpora/runs: `<output>/tmp/scrape/`; reports: `<output>/octocode-scraping/`. Chat answers stay in chat; approved source/config edits keep their paths.

Frame URL/domain, goal, depth, and output before fetching; vague scope → `references/user-inputs.md`. Default to one public URL, `--mode html`, no explicit provider (keyless `cdp`→`direct`), `.octocode/tmp/scrape/{sessionId}`, and compact stdout. Search an existing corpus before refetching. Live interaction belongs to `octocode-chrome-devtools`; process its HAR into the same session.

**Context gate:** After every fetch, the SCREEN step is mandatory — never read corpus pages in full before screening them with `clasify`. This is the primary protection against context bloat. The fallback is `corpus-find.mjs` lexical triage only when `clasify` is unavailable.

For repo, package, or code claims, use `octocode-research`. Keep URL fetching and corpus extraction in this skill.

Ask before auth, hosted spend, crawl expansion, CAPTCHA/MFA, personal-data export, form submits, purchases, sends, deletes, or account changes. Stop after two same-class failures, a hosted `403`, an auth/challenge gate, one failed CDP escalation, or enough saved evidence. Stop before expanding a crawl whose summary is not yet useful. Use `references/failure-recovery.md`; cite artifact paths plus URL metadata, never raw dumps.

## Route

- When fetching/crawling/extracting, run `scripts/fetch.mjs --url <u> [--mode html] [--crawl --same-domain --max-pages <n>] [--no-raw]`; when a brief is also needed, run `scripts/fetch-and-brief.mjs --url <u>`.
- Before routing/spend → `scripts/provider-check.mjs [--provider <p>]`; credit status → `scripts/provider-usage.mjs`. Both sanitize secrets.
- **SCREEN (mandatory — runs before any page read):** call `clasify` directly (never a wrapper script): build one `resources[] × questions[]` SemanticQuery where each saved corpus part is an unread `localFetch` resource, keep the matrix at 1–25 cells, and run `octocode clasify --input <request>.json --compact`. The runtime captures each part once, follows its own bounded `next.clasify` pagination, and returns exclusive `read`/`consider`/`skip` routes while bodies stay out of chat; retain partial, insufficient, relevant, and errored pages. Skip thin extractions and duplicate URLs without assessment, and resolve every file inside the session. Never read more than one page without first screening via clasify. If clasify is unavailable, fall back to `corpus-find.mjs` lexical triage. A route is not evidence; read deciding spans from kept files.
- Saved session navigation → `scripts/corpus-inspect.mjs --session-dir <d> [--page <n>]`; bounded text search → `scripts/corpus-find.mjs --session-dir <d> --query <t>`. Use after SCREEN to retrieve only kept pages.
- For another bounded corpus judgment, call `clasify` directly: one SemanticQuery applies every typed question to every resource; `{queries:[...]}` is only for independent matrices. Use Choice for named alternatives, one Noul for one yes/no proposition, and one Score for one ordered dimension. Leave enormous bodies unread via ordinary tool queries, retain raw answers, skip exact or settled checks, then inspect exact source for proof.
- When querying static DOM/assets/paths, run `scripts/dom-find.mjs`, `scripts/resource-list.mjs`, or `scripts/graph-navigate.mjs` with `--session-dir <d>`; live DOM stays in chrome-devtools.
- Local field proof → `scripts/corpus-run.mjs --session-dir <d> --roots cdp,extracts --regex <re>` or `--script <file>`.
- CDP bridge → `scripts/har-ingest.mjs --session-dir <d> --from-cdp-dir <run>`; reverse with `--export-packet`.
- When field names are unclear, run `scripts/schema-helper.mjs --intent "extract pricing and features"`.
- When an old transcript names `scripts/scrapingant-fetch.mjs`, `scripts/scrapingant-check.mjs`, or `scripts/scrapingant-usage.mjs`, treat them as forwarding shims and use the neutral scripts above.

Every runnable script accepts `--help`. Before changing scripts or providers, read `scripts/README.md`; shared modules live in `scripts/lib/`, vendored env resolution in `scripts/octocode-config.mjs`, and JSON contracts in `scripts/schemas/`.

After corpus-search changes, run `node --test scripts/tests/corpus-find.test.mjs`; after fetch/session changes, run `node --test scripts/tests/fetch-session.test.mjs`; after CDP client changes, run `node --test scripts/tests/cdp-client.test.mjs`. These finite local regressions need no browser or hosted provider; they do not replace a live browser check for CDP integration changes.

## References

- When scope, policy, or route is unclear, load `references/user-inputs.md`, `references/scraping-policy.md`, or `references/route-selection.md`.
- When choosing a provider, load `references/providers.md`; after hosted approval, load `references/scrapingant.md`; for human setup/vendor extension, read `docs/PROVIDERS.md` or `docs/ADDING_A_VENDOR.md`.
- When searching corpus layout/contracts, load `references/session-corpus.md` and `references/data-contract.md`; for graph/workflows, load `references/website-analysis.md`; for extraction/citations, load `references/extraction-quality.md`.
- When bridging a live browser, load `references/browser-scraping.md`; for blocked/thin/oversized output, load `references/failure-recovery.md`.
