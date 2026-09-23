---
name: octocode-scraping
description: "Use when fetching public URLs or crawling a site into a local corpus for repeated queries: docs, pricing tables, link maps, or content extraction. Clasify SCREEN trims candidates before reading; corpus-run does literal verification. Not for JS-rendered content or live interaction — use octocode-chrome-devtools."
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

- When fetching/crawling/extracting, run `scripts/fetch.mjs --url <u> [--mode html] [--crawl --same-domain --max-pages <n>] [--no-raw]`; when a brief is also needed, run `scripts/fetch-and-brief.mjs --url <u>`. **CDP escalation path:** if after SCREEN `corpus-run --regex` returns zero matches for target literals AND `has-target-data > 0.6` → content is JS-rendered; load `octocode-chrome-devtools`, run `open-browser + page-snapshot + dom-operations-check` on the URL, then bridge results back with `scripts/har-ingest.mjs --session-dir <existing-session>`. Do not start a new session — merge CDP data into the existing corpus.
- Before routing/spend → `scripts/provider-check.mjs [--provider <p>]`; credit status → `scripts/provider-usage.mjs`. Both sanitize secrets.
- **SCREEN (mandatory — runs before any page read):** call `clasify` directly (never a wrapper script): each saved `text/*.clean.part-*.md` is an unread `localFetch` resource with an absolute path; drop 0-byte files, thin extractions, and duplicate URLs/bodies first (an empty file returns `coverage:"error"` + `classificationContextEmpty`, not a verdict). Keep each matrix at resources × questions ≤ 25 cells; for more pages, repeat the same questions in up to 5 matrices under root `queries[]`. Omit `maxChars` (default 80000 covers a default 50 KB part); a lower cap truncates with no continuation (`coverage:"partial"` + `limitations`) — treat that page as unscreened. Run `octocode clasify --input <request>.json` (stdout is compact JSON; exit 0 judged, 6 = run `next.clasify` unchanged, 5 = every resource errored, 2 = invalid input). Output is `queries[].resources[].pages[]`, each with a line `scope` and `answers[questionId]` (`{noul}` or `{choice, confidence, probabilities}`); aggregate page verdicts per file and read only deciding scopes. There are no built-in routes: `route` is your own Choice question, and the runtime adds an `insufficient` label to every Choice. **Accept a `route` choice only when `confidence >= 0.5`; treat lower confidence, `insufficient`, `partial`, and errored pages as `consider`.** On `consider`: locate lines with `scripts/corpus-run.mjs --session-dir <d> --roots text --flags i --regex <term>` (returns `file`/`line`), then read or re-clasify that `startLine`/`endLine` window; `corpus-find` ranks whole pages and cannot locate spans. If clasify is unavailable, fall back to `corpus-find.mjs` lexical triage. A route is not evidence; read deciding spans from kept files.
- Saved session navigation → `scripts/corpus-inspect.mjs --session-dir <d> [--page <n>]`; bounded text search → `scripts/corpus-find.mjs --session-dir <d> --query <t>`. Use after SCREEN to retrieve only kept pages.
- For another bounded corpus judgment, call `clasify` directly: one SemanticQuery applies every typed question to every resource; `{queries:[...]}` is only for independent matrices. Use Choice for named alternatives, one Noul for one yes/no proposition, and one Score for one ordered dimension. Leave enormous bodies unread via ordinary tool queries, retain raw answers, skip exact or settled checks, then inspect exact source for proof.
- **Standard question set for web research SCREEN** — always include these unless the goal makes one irrelevant:
  - **Literal verification (not a clasify question):** after SCREEN produces `has-target-data > 0.6`, run `scripts/corpus-run.mjs --session-dir <d> --roots text --regex '<pattern>'` (e.g. `--regex '[0-9]+\.[0-9]+%|\$[0-9]+'` for fee data, `--regex 'curl|api_key|PaymentIntent'` for API code) before reading. `corpus-run` does exact JavaScript-regex matching (use `--flags i`, not `(?i)`); `corpus-find` is ranked lexical search and cannot verify literal presence. **Zero regex matches + `has-target-data > 0.6` → content exists topically but is JS-rendered; escalate to `cdp-needed` and load `octocode-chrome-devtools`.** Non-zero matches confirm the data is present — proceed to `read`.
  - `content-type` (Choice): criteria — `pricing-table` (explicit fee amounts/plan tiers), `api-reference` (endpoints/params/code), `docs-guide` (how-to with examples), `marketing` (hero/feature list), `listing` (navigation index or link list only), `other`.
  - `has-target-data` (Noul): write one specific question per goal, e.g. "Does this page contain Stripe payment fee percentages and per-transaction costs?"
  - `route` (Choice): `read` (extract now), `consider` (locate spans first), `skip` (follow links instead), `cdp-needed` (content is JS-rendered — escalate to chrome-devtools). Three questions fit 8 resources per matrix.
- **Link-routing question** — when the corpus contains extracted `links.jsonl` or `elements.jsonl`, clasify a sample of the link names/URLs as `context.value` resources (not file reads) to decide which sections to crawl next: use Choice with `crawl-section` / `spot-check` / `stop` criteria. This avoids fetching all links blindly.
- **HAR / network check** — when a CDP-ingested `cdp-network.jsonl` is in the corpus, clasify it with: `has-text-bodies` (Noul, "Do any entries contain HTML or JSON response bodies worth extracting, not just images or analytics beacons?") and `har-action` (Choice: `extract-bodies` / `navigate-more` / `skip`). A `noul < 0.2` means only binary/tracking — skip extraction.
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
