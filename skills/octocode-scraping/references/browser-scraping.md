# Browser Scraping

Load when a live browser step joins a scrape — auth, clicks, JS-only page, HAR, or perf. Why: one sessionId plus one CDP port keeps the evidence joinable.

Owner for the **cross-skill playbook** (chrome `SKILL.md` only points here). Live CDP = `octocode-chrome-devtools`; this skill owns corpus + ingest.

## Session IDs (do not mix)
| Kind | Handle | Reuse |
|---|---|---|
| Scrape corpus | `.octocode/tmp/scrape/{sessionId}/` | same id for fetch → ingest → `corpus-run` |
| Live Chrome | CDP `--port` + `--keep-tab` | same port/tab for DOM → click → measure → HAR |
| CDP artifacts | `.octocode/tmp/chrome-devtools/<run>/` | pass to `har-ingest --from-cdp-dir` |

## Playbook (one sessionId + one port)
1. **Map** — `fetch.mjs` (html, omit `--provider`) → `AGENT_INDEX.json` + `graph/graph.json`.
2. **Packet** — `har-ingest.mjs --export-packet` → `extracts/bridge-handoff.json`.
3. **Live DOM** — chrome `page-snapshot` → `dom-operations-check` (`--keep-tab`). Static only: `dom-find.mjs`.
4. **Click/fill** — same tab or `graph-actionability-check`; ask before destructive submits.
5. **Measure** — `performance`/`network`/`storage-measure-check` with `MEASURE_EXISTING=1`.
6. **Query** — `measure-query --dir|--latest`; HAR → `har-pager`; then deep HAR only if needed.
7. **Ingest** — `har-ingest.mjs --session-dir <session> --from-cdp-dir <run>` (chrome alias `har-ingest-to-scrape`).
8. **Prove** — `corpus-run.mjs --roots cdp,extracts --regex …` (alias `corpus-run-local`). No re-browser for the same body.

Zero actionability rows → chrome `actionability-diagnostics`. Emit paths/counts — never cookies/tokens.

## Ask first
Real profile, cookie bridge, CAPTCHA/MFA, destructive writes, purchases, sends, deletes, account changes.

Next: to search what ingest produced load `references/session-corpus.md`; if the page is still thin or blocked load `references/failure-recovery.md`.
