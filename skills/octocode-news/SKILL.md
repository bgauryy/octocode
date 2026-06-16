---
name: octocode-news
description: Researches what is new in AI, developer tools, web platform, security, and notable repositories. Use when the user asks for whats-new, latest updates, recent releases, tech news, AI news, changelogs, repo updates, or trend scanning.
---

# What's New — Tech Research Agent

**Goal**: Lock scope, sweep RSS + cataloged sources in parallel, research gaps, assemble a validated JSON report and an HTML report, then open the HTML in the default browser.

## Quick Input

Use defaults silently when the user did not provide a value. Only ask if the request is genuinely ambiguous.

1. **Domains**: A=AI, B=DevTools, C=Web/JS, D=Security, E=Repos. Default: all
2. **Window**: `24h` / `7d` / `14d` / `30d`. Default: `7d`
3. **Depth**: `brief` / `deep` / `comprehensive`. Default: `deep`

## Non-Negotiables

1. Run both discovery scripts before manual browsing.
2. Treat `references/sources.md` as the baseline catalog, not a suggestion.
3. Use official/product/project sources first; secondary sources validate or widen coverage.
4. Read the full canonical page before writing a kept item summary. RSS snippets are discovery-only.
5. When a source is a daily digest hub, open the dated daily post for the reporting window rather than summarizing the landing page.
6. Every kept item must include:
   - `summary` based on full-page content (120+ chars)
   - `whyImportant` (or legacy `whyInteresting`) explaining why the story matters now
   - `references` (at least one)
   - `contentEvidence.method = "full-page"` with `chars >= 200`
7. `topItems` are the hero stories — must not be duplicated inside `sections[].items`.
8. Record every checked, blocked, stale, or skipped source in `sourcesChecked` with `status`, `found`, and `notes`. Never silently drop a failed source.
9. Keep original item fields in JSON; add concise display fields (`shortTitle`, `shortDescription`) rather than replacing `title` or `summary`.
10. Low-heat stories still need useful editorial framing via `shortTitle` and `shortDescription`.
11. Deduplicate across RSS, manual browsing, and GitHub — same story from multiple sources counts once. Keep the richest version.
12. Finish only when all section JSONs exist, `meta.json` exists, repo verification (step 5) is done, HTML exists, and browser open was attempted.
13. When more than one domain is in scope, research each selected section/domain with its own subagent.
14. Run section subagents in parallel after discovery. Each subagent writes its own `{id}.json`. The coordinator owns dedupe, topItem selection, `meta.json`, and final HTML build.
15. Every subagent writes its section JSON file (`~/tmp/{ts}-sections/{id}.json`) following the `SectionPayload` schema and returns `coverageSummary`, `dedupeHints`, and `blockedOrStale` to the coordinator.
16. Subagents must not write `meta.json`, the validated JSON, or HTML files directly. Only the coordinator writes `meta.json` and runs `build-report`.

## Workflow

### 1) Lock Scope

Generate `{ts}` once as `YYYYMMDD-HHmmss` (e.g. `20260404-143000`) and reuse it for every output file path. Use defaults when missing: `domains=all`, `window=7d`, `depth=deep`.

If dependencies are missing: run `yarn install` from the monorepo root (this resolves all workspaces including skills).

### 2) Discovery (parallel)

Run both scripts in parallel — they are independent and both read `references/sources.md`:

**RSS fetch + health** — candidate pool, volume by domain, and per-feed health status in one pass:

```bash
yarn --cwd skills/octocode-news fetch-rss \
  --window {window} \
  --include-health \
  --json-out ~/tmp/{ts}-whats-new-rss.json
```

**Source catalog** — full coverage checklist (websites, RSS, repos, custom resources per domain):

```bash
yarn --cwd skills/octocode-news catalog-sources \
  --json-out ~/tmp/{ts}-whats-new-catalog.json
```

After both finish, you have:

- `rss.json` → `summary` shows volume by domain, `items` gives the candidate pool, and each `feeds[]` entry includes a `health` object (`ok`, `reason`, `validFeed`, freshness data) — broken/stale/empty feeds are flagged inline
- `catalog.json` → every website, RSS feed, repo, and custom resource to track in `sourcesChecked`

Note: `check-rss` still exists as a standalone script for targeted feed audits, but `--include-health` on `fetch-rss` covers the same ground in a single network pass.

### 3) Research & Write Per-Section JSON (parallel by section)

Create the output directory first:

```bash
mkdir -p ~/tmp/{ts}-sections/
```

**Resume protocol**: Before spawning subagents, check for existing section JSON files in `~/tmp/{ts}-sections/`. If a valid `{id}.json` already exists (has items or is a quiet stub), skip that domain — it was completed by a previous run. This allows recovery from coordinator crashes without re-doing finished domains.

Domains: `ai`, `devtools`, `web`, `security`, `repos`, optional `cross` (when a story clearly spans sections).

**Single-domain shortcut**: When only one domain is selected, skip subagent overhead — run the research pass inline as the coordinator. Write the section JSON file directly and proceed to Step 4.

**Quiet stubs for unrequested domains**: When `domains` is not `all`, write a quiet stub for each unselected primary domain so the HTML report renders every section:

```json
{ "id": "web", "name": "Web Platform", "icon": "W", "iconClass": "si-web", "quiet": true, "quietMsg": "Web platform was not in scope for this run.", "items": [], "sourcesChecked": [] }
```

When multiple domains are selected, spawn one subagent per domain. Each subagent runs the same four-step pass:

1. **Triage RSS candidates** — scan the RSS candidate pool from step 2 and mark clear top stories for full-page reads.
2. **Check non-RSS sources** — visit cataloged websites that do not expose RSS. Skip feeds flagged broken/stale by the health check.
3. **Validate repo/release claims** — use Octocode GitHub tools for release notes, merged PRs, changelogs, and repo context.
4. **Log everything** — record checked, blocked, stale, empty, and skipped sources. These become `sourcesChecked` entries.

Each subagent writes `~/tmp/{ts}-sections/{id}.json` following the `SectionPayload` schema:

```json
{
  "id": "ai",
  "name": "AI",
  "icon": "A",
  "iconClass": "si-ai",
  "quiet": false,
  "items": [ ... ],
  "sourcesChecked": [ ... ]
}
```

Required fields: `id`, `name`, `icon`, `iconClass`, `quiet`, `items`, `sourcesChecked`.
Set `quiet: true` + `quietMsg` + empty `items` when a domain has no notable items.

Each `items[]` entry follows the `Item` schema (domain, type, title, summary, heat, references, contentEvidence, etc.).
Each `sourcesChecked[]` entry follows the `SourceCheck` schema.

Each subagent also returns to the coordinator:
- `coverageSummary` — what was covered, what was quiet, what needs follow-up
- `dedupeHints[]` — URLs, repos, release pages, or products that may overlap another domain
- `blockedOrStale[]` — sources or candidate stories that need coordinator review

The coordinator waits for all selected subagents, then reads their JSON files for cross-domain dedup, topItem selection, and meta assembly.

Prefer Octocode for GitHub data, local scripts for catalog/RSS work, and direct web fetching for non-GitHub sources.

**Blocked-page fallback**: When a canonical URL cannot be fetched (challenge, paywall, timeout), use the RSS snippet + source metadata as `contentEvidence.method = "rss-snippet"` with `chars` set to actual snippet length. Log the reason in `sourcesChecked`. Do not silently drop the item — a well-sourced RSS summary with a clear `whyImportant` is better than nothing.

**Source tiers at brief depth**: When `depth=brief`, focus on P1 sources from `sources.md` only. Skip P2 sources unless P1 coverage is clearly thin (<3 items for a domain).

**Subagent stopping condition**: stop when the domain slice has met its depth floor or exhausted the cataloged sources for that domain, every checked/blocked/skipped source is logged, and the section JSON file is written.

**Subagent context-limit recovery**: If a subagent is approaching its context limit before finishing all sources, it must immediately write its section JSON with whatever items it has collected, mark remaining sources as `status: "skipped"` with `notes: "context limit"`, and return to the coordinator. Partial coverage is acceptable — the coordinator can flag thin sections in the report `tldr`.

**Coordinator stopping condition**: stop only when every selected domain subagent has finished or explicitly reported blocked status, cross-domain duplicates are resolved, `meta.json` is written, section files are updated (topItems removed from sections), and the global depth floor is met (brief: 15+, deep: 30+, comprehensive: 50+).

When changing the local tooling:

- Edit TypeScript, HTML, and CSS in `src/`
- Regenerate runnable artifacts with `yarn --cwd skills/octocode-news build:scripts`
- Treat `scripts/` as built output, not the authoring source

### 4) Coordinator: Merge, Rank, and Write Meta

After all subagents finish, the coordinator:

1. Reads every `~/tmp/{ts}-sections/{id}.json` written by subagents.
2. Resolves cross-domain duplicates (same URL, release page, repo, or product announcement appearing in multiple sections).
3. Promotes genuinely multi-domain stories into `cross` only when they span more than one section. If `cross` has items, writes `~/tmp/{ts}-sections/cross.json`.
4. Applies editorial ranking by recency, authority, shipped impact, and usefulness.
5. Selects 3-30 hero `topItems` from across all sections. Removes them from the section files they came from (hero items must not be duplicated inside sections).
6. Writes `~/tmp/{ts}-sections/meta.json` following the `ReportMeta` schema:

```json
{
  "window": "Mar 28-Apr 3, 2026",
  "windowLabel": "7d",
  "generated": "2026-04-03",
  "tldr": "High-signal developments across AI, developer tools, web platform, security, and repos.",
  "topItems": [ ... ],
  "sourcesChecked": [ ... ]
}
```

`meta.json` fields: `window`, `windowLabel`, `generated`, `tldr`, `topItems`, and optionally `sourcesChecked` for coordinator-level entries.

Section-level `sourcesChecked` stay in their section files. The `build-report` script unions them automatically.

Quality gates (apply during ranking):

- `tldr`: 2-5 sentences, 120+ chars, editorial not fragmentary
- item `title`: unique, SEO-friendly headline, accurate and non-clickbait
- item `summary`: 2-3 sentences, 120+ chars, covering who/what/where/when
- item `whyImportant`: short paragraph on why the story matters now
- combined `title` + `summary` + `whyImportant` under 150 words
- neutral, objective tone
- low-heat items carry concise `shortTitle` and `shortDescription`
- hero `topItems` must not be duplicated inside sections
- preserve `references`, `contentEvidence`, dates, source info

Presentation rules for every kept item:

- Act as a professional news editor for the displayed story copy.
- Always include structured `references`; the rendered HTML exposes a visible `Source Link` action.
- When an item has several references, collapse them behind a single refs affordance with a hover/focus tooltip list.
- Keep the HTML concise in prose but dense in signal: theme summaries, section stats, source counts, freshness, and verification cues should be scannable without opening raw JSON.
- Normalization adds a `theme` metadata field (`ai`, `tech`, `security`, `repositories`, `others`) to each item. The HTML renders sections in canonical domain order (`ai`, `devtools`, `web`, `security`, `repos`, optional `cross`), not by theme grouping.

### 5) Verify Repositories with Octocode MCP

**Skip at `brief` depth.** Brief reports prioritize speed over verification — proceed directly to Step 6.

After merge and ranking, run a verification pass on every item that references a GitHub repository. Use Octocode MCP tools to ground-truth the report data against live GitHub state.

**Scope**: all items across `topItems` and every section where `link`, `sourceUrl`, or any `references[].url` points to a `github.com` repo or release page.

**Verification checklist per repo item** (batch up to 3 queries per Octocode call):

1. **Repo exists and is public** — use `ghSearchRepos` with `owner` + repo name extracted from the URL. Confirm `status: "hasResults"`. If `empty` or `error`, flag the item.
2. **Stars and activity** — from the search result, read `stars`, `pushedAt`, `language`, `topics`. Update the item `stars` field with the real count (e.g. `"★ 12.4k"`). Flag repos with no pushes in >90 days as potentially stale.
3. **Release claims** — when an item claims a specific release version, use `ghGetFileContent` on `CHANGELOG.md`, `RELEASES.md`, or the GitHub releases page path. Verify the version string appears. Alternatively use `ghSearchPRs` with `type="metadata"` to confirm merged release PRs.
4. **README / description sanity** — use `ghViewRepoStructure` (root, depth=1) to confirm the repo has a README and is not empty/archived. Cross-check the repo description against the item summary for accuracy.

**Actions based on verification results**:

| Result | Action |
|--------|--------|
| Repo confirmed, data matches | No change — item passes |
| Stars differ by >10% | Update `stars` field with verified count |
| Release version not found | Add note to `whyImportant`, downgrade `heat` by 10 |
| Repo not found / private / archived | Remove item or move to `blockedOrStale`, log in `sourcesChecked` |
| Repo stale (no push >90d) | Add staleness note to `whyImportant`, consider lowering `heat` |

**Efficiency rules**:

- Batch 3 repos per `ghSearchRepos` call (the tool supports 1-3 queries).
- Skip verification for items where `type` is `blog`, `newsletter`, or `advisory` and no GitHub URL appears in `link` or `references`.
- Do not re-verify repos already checked by subagents in step 3 — carry forward their `sourcesChecked` entries. Only verify repos that subagents discovered via RSS or web sources without GitHub tool confirmation.
- Update the section JSON files in-place after verification. The coordinator owns this step.

### 6) Build, Validate, and Open

```bash
yarn --cwd skills/octocode-news build-report \
  --section-dir ~/tmp/{ts}-sections/ \
  --json-out ~/tmp/{ts}-whats-new.json \
  --output ~/tmp/{ts}-whats-new.html \
  --require-full-content \
  --open
```

The `--section-dir` reads `meta.json` and every `{id}.json` from the directory, merges them, validates with Zod, normalizes, then injects each section into its own `<script>` block in the HTML.

**Legacy single-file mode** still works for backwards compatibility:

```bash
yarn --cwd skills/octocode-news build-report \
  --input ~/tmp/{ts}-whats-new.raw.json \
  --json-out ~/tmp/{ts}-whats-new.json \
  --output ~/tmp/{ts}-whats-new.html \
  --require-full-content \
  --open
```

**If validation fails**: the error output lists every failing item and the reason (missing `whyImportant`, short `summary`, missing `references`, bad `contentEvidence`). Fix the failing items in the relevant section JSON and re-run. Do not skip `--require-full-content`.

Fallback only if the script itself is broken:

1. Copy `scripts/report-template.html`
2. Replace `__REPORT_META__` with the meta JSON, and each `__SECTION_{ID}__` with the section JSON (or `null`)
3. Open the HTML manually

## Reference Files

| File | Purpose | Used in |
|------|---------|---------|
| `references/sources.md` | Source catalog — baseline for all research | Steps 2, 3 |
| `references/dataStructure.md` | Schema guide + example JSON + constraints | Steps 3, 4 |
| `src/report-schema.ts` | Zod schema — `SectionPayload`, `ReportMeta`, and full `ReportData` | Steps 3, 4, 6 |
| `src/` | Editable TypeScript, HTML, and CSS source | All steps (edit here, then `build:scripts`) |
| `scripts/` | Bundled/minified runnable artifacts | Steps 2, 6 |
| `scripts/report-template.html` | Self-contained UI template (per-section `<script>` blocks, CSS inlined at build) | Step 6 (fallback) |
